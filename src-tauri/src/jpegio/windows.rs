//! JPEG encoding through the Windows Imaging Component.

use ::windows::core::{w, Interface};
use ::windows::Win32::Graphics::Imaging::*;
use ::windows::Win32::System::Com::StructuredStorage::{CreateStreamOnHGlobal, IPropertyBag2, PROPBAG2};
use ::windows::Win32::System::Com::{CoCreateInstance, IStream, CLSCTX_INPROC_SERVER, STATFLAG_NONAME, STREAM_SEEK_SET};
use ::windows::Win32::System::Variant::VARIANT;

thread_local! {
    static FACTORY: Option<IWICImagingFactory> = unsafe {
        crate::mf::init_thread();
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
    };
}

/// `bgra` is tightly packed 32-bit BGRA (alpha ignored), `quality` 1..=100.
pub fn encode_bgra(bgra: Vec<u8>, w: usize, h: usize, quality: u8) -> Option<Vec<u8>> {
    FACTORY.with(|f| encode(f.as_ref()?, &bgra, w, h, quality).ok())
}

fn encode(factory: &IWICImagingFactory, bgra: &[u8], w: usize, h: usize, quality: u8) -> ::windows::core::Result<Vec<u8>> {
    unsafe {
        let bitmap = factory.CreateBitmapFromMemory(w as u32, h as u32, &GUID_WICPixelFormat32bppBGR, (w * 4) as u32, bgra)?;
        let stream: IStream = CreateStreamOnHGlobal(Default::default(), true)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut props: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut props)?;
        let frame = frame.ok_or_else(::windows::core::Error::empty)?;
        if let Some(props) = &props {
            let name = PROPBAG2 { pstrName: ::windows::core::PWSTR(w!("ImageQuality").as_ptr() as *mut _), ..Default::default() };
            let _ = props.Write(1, &name, &VARIANT::from(quality.clamp(1, 100) as f32 / 100.0));
        }
        frame.Initialize(props.as_ref())?;
        frame.SetSize(w as u32, h as u32)?;
        frame.WriteSource(&bitmap.cast::<IWICBitmapSource>()?, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()?;

        let mut stat = Default::default();
        stream.Stat(&mut stat, STATFLAG_NONAME)?;
        let mut out = vec![0u8; stat.cbSize as usize];
        stream.Seek(0, STREAM_SEEK_SET, None)?;
        let mut read = 0u32;
        stream.Read(out.as_mut_ptr() as *mut _, out.len() as u32, Some(&mut read)).ok()?;
        out.truncate(read as usize);
        Ok(out)
    }
}
