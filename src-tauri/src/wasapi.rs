//! System audio on Windows: WASAPI loopback of the default output device, delivered
//! as interleaved stereo f32 with the performance-counter time (100 ns units) of each
//! packet's first frame, the same clock as Windows.Graphics.Capture frame times.

use ::windows::core::GUID;
use ::windows::Win32::Media::Audio::*;
use ::windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

pub struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    pub rate: u32,
    channels: usize,
    float: bool,
    bits: u16,
}

// COM objects from the multithreaded apartment
unsafe impl Send for Loopback {}

impl Loopback {
    pub fn open() -> Result<Loopback, String> {
        crate::mf::init_thread();
        let e = |what: &'static str| move |err: ::windows::core::Error| format!("{what}: {err}");
        unsafe {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(e("audio devices"))?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|_| "no audio output device".to_string())?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(e("audio device"))?;
            let fmt = client.GetMixFormat().map_err(e("audio format"))?;
            let f = *fmt;
            let float = f.wFormatTag == WAVE_FORMAT_IEEE_FLOAT
                || (f.wFormatTag == WAVE_FORMAT_EXTENSIBLE
                    && std::ptr::read_unaligned(std::ptr::addr_of!((*(fmt as *const WAVEFORMATEXTENSIBLE)).SubFormat)) == IEEE_FLOAT);
            let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, fmt, None);
            CoTaskMemFree(Some(fmt as *const _));
            r.map_err(e("audio loopback"))?;
            let capture: IAudioCaptureClient = client.GetService().map_err(e("audio capture"))?;
            client.Start().map_err(e("audio start"))?;
            Ok(Loopback { client, capture, rate: f.nSamplesPerSec, channels: f.nChannels.max(1) as usize, float, bits: f.wBitsPerSample })
        }
    }

    /// Hand every waiting packet to `f(stereo, qpc_time_hns)`. Silent packets come as zeros.
    pub fn read(&mut self, mut f: impl FnMut(&[f32], i64)) -> Result<(), String> {
        unsafe {
            loop {
                let n = self.capture.GetNextPacketSize().map_err(|e| e.to_string())?;
                if n == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                let mut qpc = 0u64;
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc))
                    .map_err(|e| e.to_string())?;
                let frames_n = frames as usize;
                let mut out = vec![0f32; frames_n * 2];
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 && !data.is_null() {
                    let ch = self.channels;
                    let sample = |i: usize| -> f32 {
                        if self.float && self.bits == 32 {
                            *(data as *const f32).add(i)
                        } else if self.bits == 16 {
                            *(data as *const i16).add(i) as f32 / 32768.0
                        } else if self.bits == 32 {
                            *(data as *const i32).add(i) as f32 / 2147483648.0
                        } else {
                            0.0
                        }
                    };
                    for i in 0..frames_n {
                        let l = sample(i * ch);
                        let r = if ch > 1 { sample(i * ch + 1) } else { l };
                        out[i * 2] = l;
                        out[i * 2 + 1] = r;
                    }
                }
                let _ = self.capture.ReleaseBuffer(frames);
                f(&out, qpc as i64);
            }
        }
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Current performance-counter time in 100 ns units (the WASAPI / capture clock).
pub fn qpc_now() -> i64 {
    use ::windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    (c as i128 * 10_000_000 / f.max(1) as i128) as i64
}
