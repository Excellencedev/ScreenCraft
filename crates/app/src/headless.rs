//! Renders the UI without a window: egui draws into an offscreen texture that is read back as
//! RGBA. Used for automated screenshots and UI regression checks; it needs only a GPU.

use eframe::egui::{self, Pos2, Rect, Vec2};
use eframe::egui_wgpu::{self, RenderState, RendererOptions, ScreenDescriptor, WgpuConfiguration};

pub struct Headless {
    pub ctx: egui::Context,
    pub state: RenderState,
    size_px: [u32; 2],
    ppp: f32,
    target: eframe::wgpu::Texture,
    time: f64,
}

impl Headless {
    pub fn new(size_px: [u32; 2], ppp: f32) -> Result<Self, String> {
        let instance = eframe::wgpu::Instance::new(eframe::wgpu::InstanceDescriptor::new_without_display_handle());
        let state = pollster_block_on(RenderState::create(&WgpuConfiguration::default(), &instance, None, RendererOptions::default()))
            .map_err(|e| format!("GPU setup failed: {e}"))?;
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let target = Self::make_target(&state, size_px);
        Ok(Self { ctx, state, size_px, ppp, target, time: 0.0 })
    }

    fn make_target(state: &RenderState, size_px: [u32; 2]) -> eframe::wgpu::Texture {
        state.device.create_texture(&eframe::wgpu::TextureDescriptor {
            label: Some("headless-target"),
            size: eframe::wgpu::Extent3d { width: size_px[0], height: size_px[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: eframe::wgpu::TextureDimension::D2,
            format: state.target_format,
            usage: eframe::wgpu::TextureUsages::RENDER_ATTACHMENT | eframe::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Run one UI pass and return its pixels (RGBA8, tightly packed).
    pub fn frame(&mut self, mut ui_fn: impl FnMut(&mut egui::Ui)) -> Vec<u8> {
        self.time += 1.0 / 60.0;
        let mut raw = egui::RawInput::default();
        raw.time = Some(self.time);
        raw.screen_rect = Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(self.size_px[0] as f32 / self.ppp, self.size_px[1] as f32 / self.ppp),
        ));
        if let Some(v) = raw.viewports.get_mut(&egui::ViewportId::ROOT) {
            v.native_pixels_per_point = Some(self.ppp);
        }
        let mut out = self.ctx.run_ui(raw, |ui| ui_fn(ui));
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let sd = ScreenDescriptor { size_in_pixels: self.size_px, pixels_per_point: out.pixels_per_point };

        let (device, queue) = (&self.state.device, &self.state.queue);
        let mut renderer = self.state.renderer.write();
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        let mut enc = device.create_command_encoder(&Default::default());
        let extra = renderer.update_buffers(device, queue, &mut enc, &jobs, &sd);
        let view = self.target.create_view(&Default::default());
        {
            let mut pass = enc
                .begin_render_pass(&eframe::wgpu::RenderPassDescriptor {
                    label: Some("headless-ui"),
                    color_attachments: &[Some(eframe::wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: eframe::wgpu::Operations {
                            load: eframe::wgpu::LoadOp::Clear(eframe::wgpu::Color::BLACK),
                            store: eframe::wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &sd);
        }
        for id in &out.textures_delta.free {
            renderer.free_texture(id);
        }
        out.textures_delta.clear();
        drop(renderer);
        queue.submit(extra.into_iter().chain(std::iter::once(enc.finish())));
        self.read_back()
    }

    fn read_back(&self) -> Vec<u8> {
        let [w, h] = self.size_px;
        let padded = (w * 4).div_ceil(256) * 256;
        let device = &self.state.device;
        let buf = device.create_buffer(&eframe::wgpu::BufferDescriptor {
            label: Some("headless-readback"),
            size: padded as u64 * h as u64,
            usage: eframe::wgpu::BufferUsages::COPY_DST | eframe::wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            eframe::wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: eframe::wgpu::Origin3d::ZERO,
                aspect: eframe::wgpu::TextureAspect::All,
            },
            eframe::wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: eframe::wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            eframe::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.state.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(eframe::wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(eframe::wgpu::PollType::wait_indefinitely());
        let _ = rx.recv();
        let mapped = slice.get_mapped_range().expect("mapped range");
        let mut out = vec![0u8; (w * h * 4) as usize];
        let bgra = matches!(
            self.state.target_format,
            eframe::wgpu::TextureFormat::Bgra8Unorm | eframe::wgpu::TextureFormat::Bgra8UnormSrgb
        );
        for y in 0..h as usize {
            let s = y * padded as usize;
            out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(&mapped[s..s + w as usize * 4]);
        }
        if bgra {
            for px in out.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        out
    }

    pub fn size(&self) -> [u32; 2] {
        self.size_px
    }
}

fn pollster_block_on<F: std::future::Future>(f: F) -> F::Output {
    // eframe already pulls pollster in; a tiny local executor keeps this file dependency-free.
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Noop(std::thread::Thread);
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Noop(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[allow(dead_code)]
fn _unused(_: egui_wgpu::Renderer) {}
