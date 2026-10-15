//! Linux backend: xdg-desktop-portal ScreenCast to pick the source, PipeWire to receive it.
//! Works on Wayland and on X11 sessions that run a portal. The portal shows its own chooser,
//! so the source list is a single "choose in the system dialog" entry.

use crate::pixels::crop_scale;
use crate::{even, CaptureRequest, Capturer, DisplayInfo, Event, Frame, Permissions, Sources, StartInfo};
use pipewire as pw;
use pw::spa;
use screencraft_audio::Clock;
use screencraft_core::project::Rect;
use std::os::fd::OwnedFd;
use std::sync::mpsc::{channel, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub fn permissions() -> Permissions {
    // The portal asks every time; nothing to pre-grant.
    Permissions { screen: true, input_monitoring: true, accessibility: true }
}

pub fn request_screen_access() -> bool {
    true
}

pub fn request_input_monitoring() -> bool {
    true
}

pub fn list_sources() -> Result<Sources, String> {
    Ok(Sources {
        displays: vec![DisplayInfo {
            id: 0,
            name: "Screen or window (choose in the system dialog)".into(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            is_main: true,
        }],
        windows: Vec::new(),
    })
}

#[derive(Default)]
pub struct PlatformCapturer {
    quit: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

struct Negotiated {
    width: u32,
    height: u32,
    format: spa::param::video::VideoFormat,
    stride_hint: u32,
}

async fn open_portal() -> Result<(u32, Option<(i32, i32)>, OwnedFd), String> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
    use ashpd::desktop::PersistMode;
    let proxy = Screencast::new().await.map_err(|e| format!("screen-cast portal: {e}"))?;
    let session = proxy.create_session(Default::default()).await.map_err(|e| e.to_string())?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                // Metadata keeps the pointer out of the picture and gives us its position.
                .set_cursor_mode(CursorMode::Metadata)
                .set_sources(SourceType::Monitor | SourceType::Window)
                .set_multiple(false)
                .set_restore_token(None)
                .set_persist_mode(PersistMode::DoNot),
        )
        .await
        .map_err(|e| e.to_string())?;
    let response = proxy
        .start(&session, None, Default::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| format!("screen sharing was cancelled ({e})"))?;
    let stream = response.streams().first().ok_or("no screen or window was selected")?.to_owned();
    let fd = proxy.open_pipe_wire_remote(&session, Default::default()).await.map_err(|e| e.to_string())?;
    Ok((stream.pipe_wire_node_id(), stream.size(), fd))
}

/// Everything the PipeWire thread needs; it runs until the quit channel fires.
struct PwArgs {
    fd: OwnedFd,
    node: u32,
    clock: Clock,
    tx: SyncSender<Event>,
    max_long_side: Option<u32>,
    fps: u32,
}

fn format_pod() -> Vec<u8> {
    use spa::param::video::VideoFormat;
    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(spa::param::format::FormatProperties::MediaType, Id, spa::param::format::MediaType::Video),
        spa::pod::property!(spa::param::format::FormatProperties::MediaSubtype, Id, spa::param::format::MediaSubtype::Raw),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle { width: 1920, height: 1080 },
            spa::utils::Rectangle { width: 16, height: 16 },
            spa::utils::Rectangle { width: 16384, height: 16384 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 60, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 240, denom: 1 }
        ),
    );
    spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj))
        .expect("serialize format pod")
        .0
        .into_inner()
}

/// Ask the compositor to attach cursor metadata (position + a small bitmap) to every buffer.
fn cursor_meta_pod() -> Vec<u8> {
    use spa::pod::{Object, Property, PropertyFlags, Value};
    let size = std::mem::size_of::<spa::sys::spa_meta_cursor>()
        + std::mem::size_of::<spa::sys::spa_meta_bitmap>()
        + 64 * 64 * 4;
    let obj = Object {
        type_: spa::sys::SPA_TYPE_OBJECT_ParamMeta,
        id: spa::sys::SPA_PARAM_Meta,
        properties: vec![
            Property {
                key: spa::sys::SPA_PARAM_META_type,
                flags: PropertyFlags::empty(),
                value: Value::Id(spa::utils::Id(spa::sys::SPA_META_Cursor)),
            },
            Property {
                key: spa::sys::SPA_PARAM_META_size,
                flags: PropertyFlags::empty(),
                value: Value::Int(size as i32),
            },
        ],
    };
    spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj))
        .expect("serialize meta pod")
        .0
        .into_inner()
}

fn pw_thread(args: PwArgs, ready: std::sync::mpsc::Sender<Result<(u32, u32), String>>, quit: pw::channel::Receiver<()>) {
    pw::init();
    let PwArgs { fd, node, clock, tx, max_long_side, fps } = args;
    let setup = || -> Result<_, pw::Error> {
        let mainloop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&mainloop, None)?;
        let core = context.connect_fd_rc(fd, None)?;
        let stream = pw::stream::StreamRc::new(
            core,
            "screencraft-capture",
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )?;
        Ok((mainloop, context, stream))
    };
    let (mainloop, _context, stream) = match setup() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(format!("PipeWire: {e}")));
            return;
        }
    };

    struct Data {
        neg: Option<Negotiated>,
        last_sent: Option<Instant>,
        ready: Option<std::sync::mpsc::Sender<Result<(u32, u32), String>>>,
    }
    let min_gap = Duration::from_secs_f64(0.8 / fps.max(1) as f64);

    let _listener = stream
        .add_local_listener_with_user_data(Data { neg: None, last_sent: None, ready: Some(ready.clone()) })
        .param_changed(|_, data, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((mt, st)) = spa::param::format_utils::parse_format(param) else { return };
            if mt != spa::param::format::MediaType::Video || st != spa::param::format::MediaSubtype::Raw {
                return;
            }
            let mut info = spa::param::video::VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }
            let size = info.size();
            data.neg = Some(Negotiated {
                width: size.width,
                height: size.height,
                format: info.format(),
                stride_hint: size.width * 4,
            });
            if let Some(r) = data.ready.take() {
                let _ = r.send(Ok((size.width, size.height)));
            }
        })
        .process(move |stream, data| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            // Cursor position rides on the buffer as metadata.
            let cursor_pos = buffer
                .find_meta::<spa::buffer::meta::MetaCursor>()
                .filter(|c| c.id() != 0)
                .map(|c| (c.position().x as f64, c.position().y as f64));
            let Some(neg) = data.neg.as_ref() else { return };
            let now = Instant::now();
            if let Some((cx, cy)) = cursor_pos {
                if neg.width > 0 && neg.height > 0 {
                    crate::input::linux::push_cursor(now, cx / neg.width as f64, cy / neg.height as f64);
                }
            }
            let datas = buffer.datas_mut();
            let Some(d) = datas.first_mut() else { return };
            let stride = {
                let s = d.chunk().stride();
                if s > 0 { s as usize } else { neg.stride_hint as usize }
            };
            let chunk_size = d.chunk().size() as usize;
            let Some(bytes) = d.data() else { return };
            let (w, h) = (neg.width as usize, neg.height as usize);
            if chunk_size == 0 || bytes.len() < stride * h || w < 2 || h < 2 {
                return;
            }
            if let Some(t) = data.last_sent {
                if t.elapsed() < min_gap {
                    return;
                }
            }
            let rgb_order = matches!(
                neg.format,
                spa::param::video::VideoFormat::RGBx | spa::param::video::VideoFormat::RGBA
            );
            let (ew, eh) = ((w & !1) as u32, (h & !1) as u32);
            let scale = match max_long_side {
                Some(m) if m > 0 => (m as f64 / ew.max(eh) as f64).min(1.0),
                _ => 1.0,
            };
            let (ow, oh) = (even(ew as f64 * scale), even(eh as f64 * scale));
            let mut bgra = crop_scale(bytes, stride, (0, 0, ew, eh), (ow, oh));
            if rgb_order {
                for px in bgra.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
            }
            // X-formats have an undefined alpha byte.
            for px in bgra.chunks_exact_mut(4) {
                px[3] = 255;
            }
            data.last_sent = Some(now);
            let _ = tx.try_send(Event::Frame(Frame { width: ow, height: oh, bgra, pts_ns: clock.now_ns() }));
        })
        .register();
    let _listener = match _listener {
        Ok(l) => l,
        Err(e) => {
            let _ = ready.send(Err(format!("PipeWire: {e}")));
            return;
        }
    };

    let fmt = format_pod();
    let meta = cursor_meta_pod();
    let (Some(fmt_pod), Some(meta_pod)) = (spa::pod::Pod::from_bytes(&fmt), spa::pod::Pod::from_bytes(&meta)) else {
        let _ = ready.send(Err("could not build stream parameters".into()));
        return;
    };
    let mut params = [fmt_pod, meta_pod];
    if let Err(e) = stream.connect(
        spa::utils::Direction::Input,
        Some(node),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    ) {
        let _ = ready.send(Err(format!("PipeWire connect: {e}")));
        return;
    }

    let _quit = quit.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });
    mainloop.run();
    let _ = stream.disconnect();
}

impl Capturer for PlatformCapturer {
    fn start(&mut self, req: &CaptureRequest, clock: Clock, tx: SyncSender<Event>) -> Result<StartInfo, String> {
        if self.thread.is_some() {
            return Err("already capturing".into());
        }
        let (node, logical, fd) = pollster::block_on(open_portal())?;
        let (quit_tx, quit_rx) = pw::channel::channel::<()>();
        let (ready_tx, ready_rx) = channel();
        let args = PwArgs {
            fd,
            node,
            clock,
            tx,
            max_long_side: req.max_long_side,
            fps: req.fps.clamp(10, 120),
        };
        let thread = std::thread::Builder::new()
            .name("screencraft-pipewire".into())
            .spawn(move || pw_thread(args, ready_tx, quit_rx))
            .map_err(|e| e.to_string())?;
        let (w, h) = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "the screen stream did not start (no frames from PipeWire)".to_string())??;
        self.quit = Some(quit_tx);
        self.thread = Some(thread);
        let scale = match req.max_long_side {
            Some(m) if m > 0 => (m as f64 / w.max(h) as f64).min(1.0),
            _ => 1.0,
        };
        let (lw, lh) = logical.map(|(a, b)| (a as f64, b as f64)).unwrap_or((w as f64, h as f64));
        Ok(StartInfo {
            width: even(w as f64 * scale),
            height: even(h as f64 * scale),
            bounds: Rect { x: 0.0, y: 0.0, width: lw, height: lh },
            display_id: None,
            provides_system_audio: false,
        })
    }

    fn stop(&mut self) {
        if let Some(q) = self.quit.take() {
            let _ = q.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PlatformCapturer {
    fn drop(&mut self) {
        self.stop();
    }
}
