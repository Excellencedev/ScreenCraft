//! The wgpu side of the compositor.

use crate::cursors::{build_atlas, CELL, HOTSPOT, KINDS, STYLES};
use crate::scene::{BackgroundKind, Scene};
use screencraft_encode::I420;
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Planes {
    y: wgpu::Texture,
    u: wgpu::Texture,
    v: wgpu::Texture,
    w: u32,
    h: u32,
}

struct Target {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    atlas_view: wgpu::TextureView,
    planes: Option<Planes>,
    bind: Option<wgpu::BindGroup>,
    target: Option<Target>,
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

impl Compositor {
    /// Use a device the caller already has (the editor shares egui's).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screencraft-compose"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compose-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compose-pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compose"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compose-uniform"),
            size: 25 * 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
        let mut c = Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            layout,
            uniform,
            sampler,
            atlas_view,
            planes: None,
            bind: None,
            target: None,
            readback: None,
        };
        c.upload(&I420::new(2, 2));
        c
    }

    /// A compositor on its own device (export, tests). Prefers a discrete/high-performance GPU.
    pub fn headless() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("GPU device: {e}"))?;
        Ok(Self::new(&device, &queue))
    }

    /// Upload the current source picture.
    pub fn upload(&mut self, f: &I420) {
        let (w, h) = (f.width, f.height);
        let need_new = !matches!(&self.planes, Some(p) if p.w == w && p.h == h);
        if need_new {
            let y = plane(&self.device, w, h, "plane-y");
            let u = plane(&self.device, w / 2, h / 2, "plane-u");
            let v = plane(&self.device, w / 2, h / 2, "plane-v");
            self.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("compose-bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&y.create_view(&Default::default())),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&u.create_view(&Default::default())),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&v.create_view(&Default::default())),
                    },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&self.atlas_view) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            }));
            self.planes = Some(Planes { y, u, v, w, h });
        }
        let p = self.planes.as_ref().unwrap();
        write_plane(&self.queue, &p.y, &f.y, w, h);
        write_plane(&self.queue, &p.u, &f.u, w / 2, h / 2);
        write_plane(&self.queue, &p.v, &f.v, w / 2, h / 2);
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if matches!(&self.target, Some(t) if t.w == w && t.h == h) {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compose-target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.target = Some(Target { tex, view, w, h });
        self.readback = None;
    }

    fn uniforms(scene: &Scene) -> [[f32; 4]; 25] {
        let mut u = [[0f32; 4]; 25];
        u[0] = [scene.out.0 as f32, scene.out.1 as f32, scene.source.0 as f32, scene.source.1 as f32];
        u[1] = scene.content;
        u[2] = [scene.radius_px, scene.shadow_blur_px, scene.shadow_intensity, scene.time_s];
        u[3] = [scene.shadow_offset[0], scene.shadow_offset[1], 0.0, 0.0];
        u[4] = [scene.camera[0], scene.camera[1], scene.camera[2], 0.0];
        u[5] = scene.crop;
        let kind = match scene.bg_kind {
            BackgroundKind::Solid => 0.0,
            BackgroundKind::Gradient => 1.0,
            BackgroundKind::Wallpaper => 2.0,
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
        u
    }

    /// Draw `scene` into the internal target and return its view (for egui) .
    pub fn render(&mut self, scene: &Scene) -> &wgpu::TextureView {
        self.ensure_target(scene.out.0, scene.out.1);
        let u = Self::uniforms(scene);
        self.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&u));
        let target = self.target.as_ref().unwrap();
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("compose") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, self.bind.as_ref().unwrap(), &[]);
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
