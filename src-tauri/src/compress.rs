//! HandBrake-style video compression using AVFoundation (hardware encoders).
//! Reads any video macOS can decode, re-encodes with a target quality, codec,
//! resolution and frame rate, and passes audio through or re-encodes it.

#![allow(deprecated)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus,
    AVURLAsset,
};
use objc2_core_media::CMTime;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_s: f64,
    pub video_kbps: f64,
    pub has_audio: bool,
    pub file_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressOpts {
    pub input: String,
    pub output: String,
    /// "h264" | "hevc"
    pub codec: String,
    /// 0..=100, higher = better quality / bigger file.
    pub quality: f64,
    /// Downscale to this height (never upscales).
    pub height: Option<u32>,
    /// Limit the frame rate (never increases it).
    pub fps: Option<f64>,
    /// "keep" | "aac160" | "aac128" | "aac96" | "aac64" | "none"
    pub audio: String,
    #[serde(default)]
    pub optimize_for_web: bool,
}

fn obj<T: objc2::Message>(r: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(r) }
}
fn num_i(v: i64) -> Retained<AnyObject> {
    obj(NSNumber::new_i64(v))
}
fn num_f(v: f64) -> Retained<AnyObject> {
    obj(NSNumber::new_f64(v))
}
fn num_b(v: bool) -> Retained<AnyObject> {
    obj(NSNumber::new_bool(v))
}
fn string(s: &str) -> Retained<AnyObject> {
    obj(NSString::from_str(s))
}
fn dict(pairs: Vec<(&str, Retained<AnyObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<Retained<NSString>> = pairs.iter().map(|(k, _)| NSString::from_str(k)).collect();
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let vals: Vec<Retained<AnyObject>> = pairs.into_iter().map(|(_, v)| v).collect();
    NSDictionary::from_retained_objects(&key_refs, &vals)
}

fn url_for(path: &str) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(path))
}

pub fn probe(path: &str) -> Result<Probe, String> {
    unsafe {
        let asset = AVURLAsset::URLAssetWithURL_options(&url_for(path), None);
        let vt = NSString::from_str("vide");
        let at = NSString::from_str("soun");
        let vtracks = asset.tracksWithMediaType(&vt);
        let vtrack = vtracks.firstObject().ok_or("no video track found")?;
        let size = vtrack.naturalSize();
        let dur = asset.duration().seconds();
        let atracks = asset.tracksWithMediaType(&at);
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Ok(Probe {
            width: size.width.round() as u32,
            height: size.height.round() as u32,
            fps: vtrack.nominalFrameRate() as f64,
            duration_s: if dur.is_finite() { dur } else { 0.0 },
            video_kbps: vtrack.estimatedDataRate() as f64 / 1000.0,
            has_audio: atracks.count() > 0,
            file_size,
        })
    }
}

fn even(v: f64) -> i64 {
    let n = v.round().max(2.0) as i64;
    n - (n % 2)
}

/// Target video bitrate from the quality slider (bits per pixel per frame).
pub fn target_bitrate(width: f64, height: f64, fps: f64, quality: f64, hevc: bool) -> f64 {
    let q = (quality / 100.0).clamp(0.0, 1.0);
    let bpp = 0.015 + q * q * 0.2;
    let bpp = if hevc { bpp * 0.62 } else { bpp };
    (width * height * fps * bpp).clamp(250_000.0, 100_000_000.0)
}

pub fn compress(opts: &CompressOpts, mut progress: impl FnMut(f64)) -> Result<(), String> {
    CANCEL.store(false, Ordering::SeqCst);
    unsafe {
        let info = probe(&opts.input)?;
        let asset = AVURLAsset::URLAssetWithURL_options(&url_for(&opts.input), None);
        let vtype = NSString::from_str("vide");
        let atype = NSString::from_str("soun");
        let vtrack = asset.tracksWithMediaType(&vtype).firstObject().ok_or("no video track")?;
        let atrack = if opts.audio == "none" { None } else { asset.tracksWithMediaType(&atype).firstObject() };

        // ---- output geometry / rate
        let (sw, sh) = (info.width as f64, info.height as f64);
        let scale = match opts.height {
            Some(h) if (h as f64) < sh => h as f64 / sh,
            _ => 1.0,
        };
        let (ow, oh) = (even(sw * scale), even(sh * scale));
        let src_fps = if info.fps > 1.0 { info.fps } else { 30.0 };
        let out_fps = opts.fps.map(|f| f.min(src_fps)).unwrap_or(src_fps);
        let hevc = opts.codec == "hevc";
        let mut bitrate = target_bitrate(ow as f64, oh as f64, out_fps, opts.quality, hevc);
        if info.video_kbps > 0.0 {
            bitrate = bitrate.min(info.video_kbps * 1000.0 * 0.98);
        }

        // ---- reader
        let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset)
            .map_err(|e| e.localizedDescription().to_string())?;
        let vout_settings = dict(vec![("PixelFormatType", num_i(875704438))]); // 420v
        let vout = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&vtrack, Some(&vout_settings));
        reader.addOutput(&vout);
        let aout = match &atrack {
            Some(t) => {
                let s = dict(vec![
                    ("AVFormatIDKey", num_i(0x6C70636D)), // 'lpcm'
                    ("AVLinearPCMBitDepthKey", num_i(16)),
                    ("AVLinearPCMIsFloatKey", num_b(false)),
                    ("AVLinearPCMIsBigEndianKey", num_b(false)),
                    ("AVLinearPCMIsNonInterleaved", num_b(false)),
                ]);
                let o = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(t, Some(&s));
                reader.addOutput(&o);
                Some(o)
            }
            None => None,
        };

        // ---- writer
        let _ = std::fs::remove_file(&opts.output);
        let file_type = NSString::from_str(if opts.output.to_lowercase().ends_with(".mov") {
            "com.apple.quicktime-movie"
        } else {
            "public.mpeg-4"
        });
        let writer = AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url_for(&opts.output), &file_type)
            .map_err(|e| e.localizedDescription().to_string())?;
        writer.setShouldOptimizeForNetworkUse(opts.optimize_for_web);

        let mut props = vec![
            ("AverageBitRate", num_f(bitrate)),
            ("MaxKeyFrameInterval", num_i((out_fps * 2.0).round() as i64)),
            ("AllowFrameReordering", num_b(true)),
        ];
        if !hevc {
            props.push(("ProfileLevel", string("H264_High_AutoLevel")));
        }
        let vsettings = dict(vec![
            ("AVVideoCodecKey", string(if hevc { "hvc1" } else { "avc1" })),
            ("AVVideoWidthKey", num_i(ow)),
            ("AVVideoHeightKey", num_i(oh)),
            ("AVVideoScalingModeKey", string("AVVideoScalingModeResizeAspect")),
            ("AVVideoCompressionPropertiesKey", obj(dict(props))),
        ]);
        let vin = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(&vtype, Some(&vsettings));
        vin.setExpectsMediaDataInRealTime(false);
        if !writer.canAddInput(&vin) {
            return Err("this codec/resolution combination is not supported by the encoder".into());
        }
        writer.addInput(&vin);

        let ain = if aout.is_some() {
            let keep = opts.audio == "keep";
            let kbps: i64 = opts.audio.strip_prefix("aac").and_then(|s| s.parse().ok()).unwrap_or(128);
            let s = dict(vec![
                ("AVFormatIDKey", num_i(0x61616320)), // 'aac '
                ("AVNumberOfChannelsKey", num_i(2)),
                ("AVSampleRateKey", num_f(48000.0)),
                ("AVEncoderBitRateKey", num_i(if keep { 192 } else { kbps } * 1000)),
            ]);
            let i = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(&atype, Some(&s));
            i.setExpectsMediaDataInRealTime(false);
            writer.addInput(&i);
            Some(i)
        } else {
            None
        };

        if !reader.startReading() {
            return Err(reader
                .error()
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "cannot read input".into()));
        }
        if !writer.startWriting() {
            return Err(writer
                .error()
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "cannot start writer".into()));
        }
        writer.startSessionAtSourceTime(CMTime::new(0, 600));

        // ---- pump samples from reader to writer
        let frame_dt = 1.0 / out_fps;
        let mut next_keep = 0.0f64;
        let mut v_done = false;
        let mut a_done = ain.is_none();
        let total = info.duration_s.max(0.001);
        let mut last_report = 0.0;
        while !(v_done && a_done) {
            if CANCEL.load(Ordering::SeqCst) {
                reader.cancelReading();
                writer.cancelWriting();
                let _ = std::fs::remove_file(&opts.output);
                return Err("cancelled".into());
            }
            let mut did = false;
            if !v_done && vin.isReadyForMoreMediaData() {
                loop {
                    match vout.copyNextSampleBuffer() {
                        Some(sb) => {
                            let pts = sb.presentation_time_stamp().seconds();
                            if pts + 1e-4 >= next_keep || out_fps >= src_fps - 0.01 {
                                if !vin.appendSampleBuffer(&sb) {
                                    break;
                                }
                                next_keep = pts + frame_dt * 0.98;
                            }
                            if pts - last_report > 0.25 {
                                last_report = pts;
                                progress((pts / total).clamp(0.0, 1.0));
                            }
                            did = true;
                            if !vin.isReadyForMoreMediaData() {
                                break;
                            }
                        }
                        None => {
                            vin.markAsFinished();
                            v_done = true;
                            break;
                        }
                    }
                }
            }
            if let (Some(ain), Some(aout)) = (&ain, &aout) {
                if !a_done && ain.isReadyForMoreMediaData() {
                    loop {
                        match aout.copyNextSampleBuffer() {
                            Some(sb) => {
                                if !ain.appendSampleBuffer(&sb) {
                                    break;
                                }
                                did = true;
                                if !ain.isReadyForMoreMediaData() {
                                    break;
                                }
                            }
                            None => {
                                ain.markAsFinished();
                                a_done = true;
                                break;
                            }
                        }
                    }
                }
            }
            if writer.status() == AVAssetWriterStatus::Failed || reader.status() == AVAssetReaderStatus::Failed {
                break;
            }
            if !did {
                std::thread::sleep(Duration::from_millis(3));
            }
        }

        writer.finishWriting();
        if writer.status() != AVAssetWriterStatus::Completed {
            let msg = writer
                .error()
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "encoding failed".into());
            return Err(msg);
        }
        progress(1.0);
        Ok(())
    }
}
