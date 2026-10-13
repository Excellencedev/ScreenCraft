//! Windows.Graphics.Capture: frames of a display or a window as D3D11 textures, on a
//! device we create with video support so the same textures can go straight into the
//! video processor and the hardware encoder.

use ::windows::core::{factory, Interface};
use ::windows::Foundation::TypedEventHandler;
use ::windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use ::windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use ::windows::Graphics::DirectX::DirectXPixelFormat;
use ::windows::Win32::Foundation::{HMODULE, HWND};
use ::windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
use ::windows::Win32::Graphics::Direct3D11::*;
use ::windows::Win32::Graphics::Dxgi::IDXGIDevice;
use ::windows::Win32::Graphics::Gdi::HMONITOR;
use ::windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use ::windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use std::sync::{Arc, Mutex};

pub enum Target {
    Display(HMONITOR),
    Window(HWND),
}

/// A captured frame: the texture, the size of the picture inside it, and its
/// capture time (performance counter, 100 ns units).
pub struct Frame<'a> {
    pub texture: &'a ID3D11Texture2D,
    pub size: (u32, u32),
    pub time: i64,
}

pub fn create_device() -> Result<ID3D11Device, String> {
    unsafe {
        let mut device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(|e| format!("Direct3D: {e}"))?;
        let device = device.ok_or("Direct3D device")?;
        // capture callbacks, the encoder and Media Foundation all use the device
        if let Ok(mt) = device.cast::<ID3D11Multithread>() {
            let _ = mt.SetMultithreadProtected(true);
        }
        Ok(device)
    }
}

/// WinRT Direct3D devices are agile (usable from any thread); windows-rs just
/// doesn't mark them `Send`.
struct Agile(IDirect3DDevice);
unsafe impl Send for Agile {}

pub struct Source {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
}

impl Source {
    /// Start capturing; `on_frame` runs on a capture thread for every new frame.
    pub fn start(
        device: &ID3D11Device,
        target: Target,
        on_frame: impl FnMut(Frame) + Send + 'static,
    ) -> Result<Source, String> {
        let e = |what: &'static str| move |err: ::windows::core::Error| format!("{what}: {err}");
        unsafe {
            let dxgi: IDXGIDevice = device.cast().map_err(e("capture device"))?;
            let winrt_device: IDirect3DDevice =
                CreateDirect3D11DeviceFromDXGIDevice(&dxgi).map_err(e("capture device"))?.cast().map_err(e("capture device"))?;
            let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(e("screen capture"))?;
            let item: GraphicsCaptureItem = match target {
                Target::Display(m) => interop.CreateForMonitor(m).map_err(e("capture display"))?,
                Target::Window(w) => interop.CreateForWindow(w).map_err(e("capture window"))?,
            };
            let size = item.Size().map_err(e("capture size"))?;
            let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt_device, format, 3, size).map_err(e("frame pool"))?;
            let session = pool.CreateCaptureSession(&item).map_err(e("capture session"))?;
            let _ = session.SetIsCursorCaptureEnabled(false);
            // Windows 11: no yellow border around what is being recorded
            let _ = session.SetIsBorderRequired(false);

            let on_frame = Arc::new(Mutex::new(on_frame));
            let pool_size = Arc::new(Mutex::new(size));
            let device2 = Agile(winrt_device.clone());
            pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, ::windows::core::IInspectable>::new(
                move |pool, _| {
                    let device2 = &device2; // capture the Send wrapper, not its field
                    let Some(pool) = pool.as_ref() else { return Ok(()) };
                    let frame = pool.TryGetNextFrame()?;
                    let content = frame.ContentSize()?;
                    let time = frame.SystemRelativeTime()?.Duration;
                    let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
                    let texture: ID3D11Texture2D = access.GetInterface()?;
                    (on_frame.lock().unwrap())(Frame {
                        texture: &texture,
                        size: (content.Width.max(0) as u32, content.Height.max(0) as u32),
                        time,
                    });
                    drop(texture);
                    let _ = frame.Close();
                    // a window that grew needs bigger buffers
                    let mut ps = pool_size.lock().unwrap();
                    if content.Width > ps.Width || content.Height > ps.Height {
                        *ps = content;
                        pool.Recreate(&device2.0, format, 3, content)?;
                    }
                    Ok(())
                },
            ))
            .map_err(e("frame events"))?;
            session.StartCapture().map_err(e("start capture"))?;
            Ok(Source { session, pool })
        }
    }

    pub fn stop(self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
