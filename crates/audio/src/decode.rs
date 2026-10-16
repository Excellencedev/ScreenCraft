//! Decode an audio file to 48 kHz interleaved stereo `f32` (music clips, imported sounds).
//! macOS uses the system decoder (Core Audio), which also covers AAC / M4A under Apple's licence.

use std::path::Path;

pub const RATE: u32 = 48_000;

/// Decode the whole file. The result is interleaved L/R at 48 kHz.
pub fn decode_file(path: &Path) -> Result<Vec<f32>, String> {
    imp::decode(path)
}

pub fn probe_duration_ms(path: &Path) -> Option<f64> {
    let pcm = decode_file(path).ok()?;
    if pcm.is_empty() {
        return None;
    }
    Some(pcm.len() as f64 / 2.0 / RATE as f64 * 1000.0)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;
    use std::path::Path;

    type OSStatus = i32;
    type ExtAudioFileRef = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct AudioStreamBasicDescription {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C)]
    struct AudioBuffer {
        channels: u32,
        byte_size: u32,
        data: *mut c_void,
    }

    #[repr(C)]
    struct AudioBufferList {
        n: u32,
        buffers: [AudioBuffer; 1],
    }

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn ExtAudioFileOpenURL(url: *const c_void, out: *mut ExtAudioFileRef) -> OSStatus;
        fn ExtAudioFileSetProperty(f: ExtAudioFileRef, prop: u32, size: u32, data: *const c_void) -> OSStatus;
        fn ExtAudioFileRead(f: ExtAudioFileRef, frames: *mut u32, list: *mut AudioBufferList) -> OSStatus;
        fn ExtAudioFileDispose(f: ExtAudioFileRef) -> OSStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(alloc: *const c_void, buf: *const u8, len: isize, is_dir: u8) -> *const c_void;
        fn CFRelease(o: *const c_void);
    }

    const K_CLIENT_DATA_FORMAT: u32 = 0x6366_6d74; // 'cfmt'
    const LINEAR_PCM: u32 = 0x6c70_636d; // 'lpcm'
    const FLAG_FLOAT_PACKED: u32 = 1 | 8; // kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked

    pub fn decode(path: &Path) -> Result<Vec<f32>, String> {
        let bytes = path.to_str().ok_or("bad path")?.as_bytes().to_vec();
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bytes.as_ptr(), bytes.len() as isize, 0);
            if url.is_null() {
                return Err("bad path".into());
            }
            let mut file: ExtAudioFileRef = std::ptr::null_mut();
            let st = ExtAudioFileOpenURL(url, &mut file);
            CFRelease(url);
            if st != 0 || file.is_null() {
                return Err(format!("This audio format cannot be read ({st})"));
            }
            let fmt = AudioStreamBasicDescription {
                sample_rate: super::RATE as f64,
                format_id: LINEAR_PCM,
                format_flags: FLAG_FLOAT_PACKED,
                bytes_per_packet: 8,
                frames_per_packet: 1,
                bytes_per_frame: 8,
                channels_per_frame: 2,
                bits_per_channel: 32,
                reserved: 0,
            };
            let st = ExtAudioFileSetProperty(file, K_CLIENT_DATA_FORMAT, std::mem::size_of::<AudioStreamBasicDescription>() as u32, &fmt as *const _ as *const c_void);
            if st != 0 {
                ExtAudioFileDispose(file);
                return Err(format!("Could not set up the audio decoder ({st})"));
            }
            let mut out: Vec<f32> = Vec::new();
            let mut buf = vec![0f32; 16384 * 2];
            loop {
                let mut frames: u32 = 16384;
                let mut list = AudioBufferList { n: 1, buffers: [AudioBuffer { channels: 2, byte_size: (frames * 8), data: buf.as_mut_ptr() as *mut c_void }] };
                let st = ExtAudioFileRead(file, &mut frames, &mut list);
                if st != 0 {
                    ExtAudioFileDispose(file);
                    return Err(format!("Audio decoding failed ({st})"));
                }
                if frames == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..frames as usize * 2]);
            }
            ExtAudioFileDispose(file);
            Ok(out)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    pub fn decode(path: &Path) -> Result<Vec<f32>, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|_| "This audio format cannot be read on this platform (MP3, FLAC, Ogg and WAV are supported).".to_string())?;
        let mut format = probed.format;
        let track = format.default_track().ok_or("no audio track")?.clone();
        let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| e.to_string())?;
        let rate = track.codec_params.sample_rate.unwrap_or(48_000);
        let mut interleaved: Vec<f32> = Vec::new();
        let mut channels = 2usize;
        while let Ok(packet) = format.next_packet() {
            if packet.track_id() != track.id {
                continue;
            }
            let Ok(decoded) = decoder.decode(&packet) else { continue };
            let spec = *decoded.spec();
            channels = spec.channels.count().max(1);
            let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sb.copy_interleaved_ref(decoded);
            interleaved.extend_from_slice(sb.samples());
        }
        // to stereo
        let frames = interleaved.len() / channels;
        let mut stereo = Vec::with_capacity(frames * 2);
        for f in 0..frames {
            let l = interleaved[f * channels];
            let r = if channels > 1 { interleaved[f * channels + 1] } else { l };
            stereo.push(l);
            stereo.push(r);
        }
        if rate == super::RATE {
            return Ok(stereo);
        }
        // linear resample to 48 kHz
        let ratio = rate as f64 / super::RATE as f64;
        let out_frames = (frames as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(out_frames * 2);
        for i in 0..out_frames {
            let pos = i as f64 * ratio;
            let (a, frac) = (pos as usize, (pos - pos.floor()) as f32);
            let b = (a + 1).min(frames.saturating_sub(1));
            for c in 0..2 {
                out.push(stereo[a * 2 + c] * (1.0 - frac) + stereo[b * 2 + c] * frac);
            }
        }
        Ok(out)
    }
}
