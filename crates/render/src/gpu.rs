//! The wgpu side of the compositor: a main pass (background, screen, cursor ...), a camera pass,
//! a CPU-overlay pass and a transition pass, all into one texture the editor shows and the
//! exporter reads back.

use crate::cursors::{build_atlas, CELL, HOTSPOT, KINDS, STYLES};
use crate::scene::{BackgroundKind, Scene};
use screencraft_encode::I420;
use std::sync::Arc;
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Which picture an `I420` frame is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Screen = 0,
    /// The other clip's picture during a cross-dissolve.
    Screen2 = 1,
    Camera = 2,
    Camera2 = 3,
}

struct Planes {
    y: wgpu::Texture,
    u: wgpu::Texture,
    v: wgpu::Texture,
    yv: wgpu::TextureView,
    uv: wgpu::TextureView,
    vv: wgpu::TextureView,
    w: u32,
    h: u32,
}

struct Target {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

struct Layer {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    main_layout: wgpu::BindGroupLayout,
    main_pipeline: wgpu::RenderPipeline,
    post_pipeline: wgpu::RenderPipeline,
    cam_layout: wgpu::BindGroupLayout,
    cam_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    cam_uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    atlas_view: wgpu::TextureView,
    slots: [Option<Planes>; 4],
    dummy: Planes,
    clear_layer: wgpu::TextureView,
    layers: [Option<Layer>; 3],
    bg_tex: Option<(Arc<(u32, u32, Vec<u8>)>, wgpu::Texture, wgpu::TextureView)>,
    target: Option<Target>,
    work: Option<Target>,
    readback: Option<(wgpu::Buffer, u32)>,
}

fn plane(device: &wgpu::Device, w: u32, h: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_plane(queue: &wgpu::Queue, tex: &wgpu::Texture, data: &[u8], w: u32, h: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

fn shader_source() -> String {
    let mut hs = String::from("var<private> HOTSPOTS = array<vec2<f32>, 9>(");
    for (i, (x, y)) in HOTSPOT.iter().enumerate() {
        if i > 0 {
            hs.push_str(", ");
        }
        hs.push_str(&format!("vec2<f32>({x:?}, {y:?})"));
    }
    hs.push_str(");");
    include_str!("shader.wgsl").replace("//HOTSPOTS//", &hs)
}

fn make_planes(device: &wgpu::Device, w: u32, h: u32) -> Planes {
    let y = plane(device, w, h, "plane-y");
    let u = plane(device, w / 2, h / 2, "plane-u");
    let v = plane(device, w / 2, h / 2, "plane-v");
    Planes { yv: y.create_view(&Default::default()), uv: u.create_view(&Default::default()), vv: v.create_view(&Default::default()), y, u, v, w, h }
}

fn be<'a>(binding: u32, v: &'a wgpu::TextureView) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(v) }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, fs: &str, blend: Option<wgpu::BlendState>, label: &str) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Compositor {
    /// Use a device the caller already has (the editor shares egui's).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let main_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("screencraft-compose"), source: wgpu::ShaderSource::Wgsl(shader_source().into()) });
        let cam_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("screencraft-camera"), source: wgpu::ShaderSource::Wgsl(include_str!("camera.wgsl").into()) });
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("screencraft-blit"), source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()) });
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compose-layout"),
            entries: &[uniform_entry(0), tex_entry(1), tex_entry(2), tex_entry(3), tex_entry(4), sampler_entry(5), tex_entry(6), tex_entry(7), tex_entry(8), tex_entry(9), tex_entry(10), tex_entry(11), tex_entry(12)],
        });
        let cam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-layout"),
            entries: &[uniform_entry(0), tex_entry(1), tex_entry(2), tex_entry(3), sampler_entry(4), tex_entry(5), tex_entry(6), tex_entry(7)],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("blit-layout"), entries: &[tex_entry(0), sampler_entry(1)] });
        let main_pipeline = pipeline(device, &main_layout, &main_module, "fs", None, "compose");
        let post_pipeline = pipeline(device, &main_layout, &main_module, "fs_post", None, "compose-post");
        let cam_pipeline = pipeline(device, &cam_layout, &cam_module, "fs", Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), "camera");
        let blit_pipeline = pipeline(device, &blit_layout, &blit_module, "fs", Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), "blit");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor { label: Some("compose-uniform"), size: 48 * 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let cam_uniform = device.create_buffer(&wgpu::BufferDescriptor { label: Some("camera-uniform"), size: 8 * 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let (aw, ah, atlas) = build_atlas();
        debug_assert_eq!((aw, ah), (KINDS * CELL, STYLES * CELL));
        let atlas_tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("cursor-atlas"),
                size: wgpu::Extent3d { width: aw, height: ah, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas,
        );
        let atlas_view = atlas_tex.create_view(&Default::default());
        let clear_tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("clear-layer"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[0, 0, 0, 0],
        );
        let clear_layer = clear_tex.create_view(&Default::default());
        let dummy = make_planes(device, 2, 2);
        for (p, v) in [(&dummy.y, 16u8), (&dummy.u, 128), (&dummy.v, 128)] {
            write_plane(queue, p, &[v; 1], 1, 1);
        }
        Self {
            device: device.clone(),
            queue: queue.clone(),
            main_layout,
            main_pipeline,
            post_pipeline,
            cam_layout,
            cam_pipeline,
            blit_layout,
            blit_pipeline,
            uniform,
            cam_uniform,
            sampler,
            atlas_view,
            slots: [None, None, None, None],
            dummy,
            clear_layer,
            layers: [None, None, None],
            bg_tex: None,
            target: None,
            work: None,
            readback: None,
        }
    }

    /// A compositor on its own device (export, tests). Prefers a discrete/high-performance GPU.
    pub fn headless() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("GPU device: {e}"))?;
        Ok(Self::new(&device, &queue))
    }

    /// Upload the current screen picture.
    pub fn upload(&mut self, f: &I420) {
        self.upload_slot(Slot::Screen, f);
    }

    pub fn upload_slot(&mut self, slot: Slot, f: &I420) {
        let (w, h) = (f.width & !1, f.height & !1);
        let i = slot as usize;
        let need_new = !matches!(&self.slots[i], Some(p) if p.w == w && p.h == h);
        if need_new {
            self.slots[i] = Some(make_planes(&self.device, w, h));
        }
        let p = self.slots[i].as_ref().unwrap();
        write_plane(&self.queue, &p.y, &f.y, w, h);
        write_plane(&self.queue, &p.u, &f.u, w / 2, h / 2);
        write_plane(&self.queue, &p.v, &f.v, w / 2, h / 2);
    }

    /// Forget a slot's picture (nothing to blend / no camera).
    pub fn clear_slot(&mut self, slot: Slot) {
        self.slots[slot as usize] = None;
    }

    pub fn has_slot(&self, slot: Slot) -> bool {
        self.slots[slot as usize].is_some()
    }

    fn ensure_target(device: &wgpu::Device, t: &mut Option<Target>, w: u32, h: u32, label: &str) {
        if matches!(t, Some(x) if x.w == w && x.h == h) {
            return;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        *t = Some(Target { tex, view, w, h });
    }

    fn upload_layer(&mut self, i: usize, px: &tiny_skia::Pixmap) {
        let (w, h) = (px.width(), px.height());
        if !matches!(&self.layers[i], Some(l) if l.w == w && l.h == h) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("overlay-layer"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.layers[i] = Some(Layer { tex, view, w, h });
        }
        let l = self.layers[i].as_ref().unwrap();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &l.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            px.data(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    fn upload_bg(&mut self, img: &Arc<(u32, u32, Vec<u8>)>) {
        if matches!(&self.bg_tex, Some((a, _, _)) if Arc::ptr_eq(a, img)) {
            return;
        }
        let (w, h, data) = (img.0, img.1, &img.2);
        let tex = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("bg-image"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );
        let view = tex.create_view(&Default::default());
        self.bg_tex = Some((img.clone(), tex, view));
    }

    fn uniforms(scene: &Scene) -> [[f32; 4]; 48] {
        let mut u = [[0f32; 4]; 48];
        u[0] = [scene.out.0 as f32, scene.out.1 as f32, scene.source.0 as f32, scene.source.1 as f32];
        u[1] = scene.screen;
        u[2] = [scene.radius_px, scene.shadow_blur_px, scene.shadow_intensity, scene.time_s];
        u[4] = [scene.camera[0], scene.camera[1], scene.camera[2], 0.0];
        u[5] = scene.crop;
        let kind = match scene.bg_kind {
            BackgroundKind::Solid => 0.0,
            BackgroundKind::Gradient => 1.0,
            BackgroundKind::Wallpaper => 2.0,
            BackgroundKind::Image => 3.0,
        };
        u[6] = [kind, scene.bg_angle_rad, if scene.bg_drift { 1.0 } else { 0.0 }, 0.0];
        u[7] = scene.bg0;
        u[8] = scene.bg1;
        for i in 0..3 {
            u[9 + i] = [scene.blobs[i][0], scene.blobs[i][1], scene.blobs[i][2], 0.0];
            u[12 + i] = scene.blob_colors[i];
        }
        if let Some(c) = &scene.cursor {
            u[15] = [c.pos[0], c.pos[1], c.size_px, c.rotation];
            u[16] = [c.alpha, c.scale, c.kind as f32, c.style as f32];
        }
        u[17] = [scene.highlight_radius_px, scene.highlight_alpha, 0.0, 0.0];
        u[18] = scene.highlight_color;
        for i in 0..4 {
            u[19 + i] = match scene.ripples.get(i) {
                Some(r) => [r.pos[0], r.pos[1], r.progress, if r.ring { 1.0 } else { 2.0 }],
                None => [0.0, 0.0, -1.0, 0.0],
            };
        }
        u[23] = scene.ripple_color;
        u[24] = [scene.points_to_px, scene.ripple_line_px, 0.0, 0.0];
        u[25] = scene.shadow_box;
        u[3] = [scene.shadow_offset[0], scene.shadow_offset[1], scene.shadow_radius_px, 0.0];
        u[26] = [scene.screen_alpha, scene.dissolve, scene.bg_motion as f32, scene.bg_blur_px];
        u[27] = [
            if scene.layers.under.is_some() { 1.0 } else { 0.0 },
            if scene.layers.content.is_some() { 1.0 } else { 0.0 },
            0.0,
            scene.layers.effects.len().min(8) as f32,
        ];
        if let Some(img) = &scene.bg_image {
            u[28] = [img.0 as f32, img.1 as f32, 0.0, 0.0];
        }
        for (i, e) in scene.layers.effects.iter().take(8).enumerate() {
            u[29 + i] = e.rect;
            u[37 + i] = [e.kind as f32, e.cell_px, e.radius_px, 0.0];
        }
        if let Some((k, p)) = scene.transition {
            u[44] = [k as f32, p, 0.0, 0.0];
        }
        u
    }

    fn main_bind(&self, work: &wgpu::TextureView) -> wgpu::BindGroup {
        let s0 = self.slots[Slot::Screen as usize].as_ref().unwrap_or(&self.dummy);
        let s1 = self.slots[Slot::Screen2 as usize].as_ref().unwrap_or(&self.dummy);
        let view = |i: usize| self.layers[i].as_ref().map(|l| &l.view).unwrap_or(&self.clear_layer);
        let bg = self.bg_tex.as_ref().map(|b| &b.2).unwrap_or(&self.clear_layer);
        let e = be;
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose-bind"),
            layout: &self.main_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                e(1, &s0.yv),
                e(2, &s0.uv),
                e(3, &s0.vv),
                e(4, &self.atlas_view),
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                e(6, &s1.yv),
                e(7, &s1.uv),
                e(8, &s1.vv),
                e(9, view(0)),
                e(10, view(1)),
                e(11, bg),
                e(12, work),
            ],
        })
    }

    /// Draw `scene` into the internal target and return its view (for egui).
    pub fn render(&mut self, scene: &Scene) -> &wgpu::TextureView {
        let (w, h) = scene.out;
        Self::ensure_target(&self.device, &mut self.target, w, h, "compose-target");
        let post = scene.transition.is_some();
        if post {
            Self::ensure_target(&self.device, &mut self.work, w, h, "compose-work");
        }
        // CPU layers
        match &scene.layers.under {
            Some(p) => self.upload_layer(0, p),
            None => self.layers[0] = None,
        }
        match &scene.layers.content {
            Some(p) => self.upload_layer(1, p),
            None => self.layers[1] = None,
        }
        match &scene.layers.top {
            Some(p) => self.upload_layer(2, p),
            None => self.layers[2] = None,
        }
        if let Some(img) = &scene.bg_image {
            self.upload_bg(img);
        }
        let u = Self::uniforms(scene);
        self.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&u));

        // camera uniform
        let cam_planes = self.slots[Slot::Camera as usize].as_ref();
        let cam = scene.cam.filter(|_| cam_planes.is_some());
        if let (Some(c), Some(cp)) = (cam, cam_planes) {
            let c2 = self.slots[Slot::Camera2 as usize].as_ref();
            let blend2 = if c2.is_some() { c.blend2 } else { 0.0 };
            let mut cu = [[0f32; 4]; 8];
            cu[0] = [w as f32, h as f32, cp.w as f32, cp.h as f32];
            cu[1] = c.rect;
            cu[2] = [c.radius, c.alpha, c.shadow, c.border];
            cu[3] = c.border_color;
            cu[4] = [c.mirror, h as f32 / 1080.0, blend2, c2.map(|p| p.w as f32).unwrap_or(1.0)];
            cu[5] = [c2.map(|p| p.h as f32).unwrap_or(1.0), 0.0, 0.0, 0.0];
            self.queue.write_buffer(&self.cam_uniform, 0, bytemuck::cast_slice(&cu));
        }

        let main_view_is_work = post;
        let work_view_for_bind = if post { &self.work.as_ref().unwrap().view } else { &self.target.as_ref().unwrap().view };
        // The main pass never samples `work`; bind a harmless texture there.
        let dummy_bind = self.main_bind(&self.clear_layer);
        let _ = work_view_for_bind;
        let main_target = if main_view_is_work { &self.work.as_ref().unwrap().view } else { &self.target.as_ref().unwrap().view };

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("compose") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose-main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: main_target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.main_pipeline);
            pass.set_bind_group(0, &dummy_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if let (Some(_), Some(cp)) = (cam, cam_planes) {
            let c2 = self.slots[Slot::Camera2 as usize].as_ref().unwrap_or(&self.dummy);
            let e = be;
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("camera-bind"),
                layout: &self.cam_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.cam_uniform.as_entire_binding() },
                    e(1, &cp.yv),
                    e(2, &cp.uv),
                    e(3, &cp.vv),
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    e(5, &c2.yv),
                    e(6, &c2.uv),
                    e(7, &c2.vv),
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose-camera"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: main_target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.cam_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if let Some(top) = &self.layers[2] {
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit-bind"),
                layout: &self.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&top.view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose-top"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: main_target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if post {
            let bind = self.main_bind(&self.work.as_ref().unwrap().view);
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose-post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target.as_ref().unwrap().view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        &self.target.as_ref().unwrap().view
    }

    pub fn target_view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|t| &t.view)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Read the last rendered frame back as tightly packed RGBA8. Blocks until the GPU is done.
    pub fn read_rgba(&mut self) -> Result<Vec<u8>, String> {
        let (w, h) = {
            let t = self.target.as_ref().ok_or("nothing rendered yet")?;
            (t.w, t.h)
        };
        let padded = (w * 4).div_ceil(256) * 256;
        let need = matches!(&self.readback, Some((_, p)) if *p == padded);
        if !need {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: padded as u64 * h as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.readback = Some((buf, padded));
        }
        let (buf, _) = self.readback.as_ref().unwrap();
        let t = self.target.as_ref().unwrap();
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("GPU wait: {e:?}"))?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("GPU readback: {e}"))?;
        let mapped = slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?;
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            let s = y * padded as usize;
            out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(&mapped[s..s + w as usize * 4]);
        }
        drop(mapped);
        buf.unmap();
        Ok(out)
    }
}
