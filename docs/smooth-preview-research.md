# Smooth, freeze-free editor preview: research and recommendation

Scope: ScreenCraft editor (Tauri 2.12 / wry 0.57, WKWebView, vanilla JS, macOS 15+, dev machine macOS 26.6).
Written 2026-10-12. Items marked (UNVERIFIED) are my inference or could not be confirmed from a source.

## 0. What the repo does today (grounding)

- `ui/js/editor/playback.js`: drives `media.screen` (2880x1800 SCK mp4), `media.camera`, `media.mic`, plus one `<audio>` per music clip. Seeks one `<video>` at a time (`currentTime`/`fastSeek`), re-seeks secondaries when drift > 150-350 ms, `settle()`s all elements before `play()`. The code comments already document that `playbackRate` changes and mid-play seeks deadlocked WebKit's AVFoundation player. `S.proxy` ("in-memory scrub frames") exists but is never populated.
- `ui/js/editor/draw.js` + `ui/js/render.js`: every rAF, `paintFrame()` does `ctx.drawImage(video, sx, sy, sw, sh, ...)` into a 2D canvas, with a cached static background layer, rounded clip, cursor, click rings, camera bubble. Resolution is adaptively scaled while playing/scrubbing.
- `ui/js/export.js`: seeks `<video>` per frame and calls the same `paintFrame()`, encodes with WebCodecs `VideoEncoder`, muxes with mp4-muxer, writes via chunked `invoke("export_write")`.
- Rust already has: `compress.rs` (AVAssetReader + AVAssetWriter via objc2-av-foundation 0.3.2, hardware encode), `symphonia 0.6.1` (aac/isomp4/mp3/wav), `compute_render_track` (per-frame zoom/camera/cursor), asset protocol enabled, `macOS minimumSystemVersion 15.0`.

Root cause per your sample: the web process does a *synchronous* IPC (`MediaPlayerPrivateRemote::videoFrameForCurrentTime`) to the GPU process on every `drawImage(video)`; the GPU process main thread owns all AVPlayers of all media elements, so any stall in one (`AVPlayerItem currentTime` under `rateChanged`) blocks the web content main thread = UI freeze. The structural fix is to stop having N AVPlayers behind the web view being poked from rAF, not to tune the pokes.

## 1. Option A: decode natively in Rust, present natively or ship frames

### 1a. Native layer (Metal/CALayer/IOSurface) under or over the WKWebView
- Feasible mechanically: wry supports a transparent WKWebView (`transparent` feature; 0.54.2 release notes: background_color implemented, drawsBackground disabled) https://v2.tauri.app/release/wry/v0.54.2/ . Tauri exposes `ns_window()`; you can add a sibling `NSView` with an `AVSampleBufferDisplayLayer`/`CAMetalLayer` below the webview via objc2-app-kit. I found **no shipping Tauri example** that does this (searched; only generic par-term Metal-layer code https://docs.rs/crate/par-term/0.40.0/source/src/macos_metal.rs). Treat as unproven here.
- The real problem is compositing, not display: your picture is background + shadow + rounded clip + zoom crop + cursor + camera bubble, and the same look must match `paintFrame()` used by export. A native layer under a transparent webview only works if the *entire* composite moves native (re-implement render.js in Metal/wgpu: rounded-rect clip, shadow, gradient/wallpaper, cursor sprites, click rings, text-free). That is a rewrite plus a second renderer that must stay pixel-matched with JS export (or export also moves native: bigger win, bigger job). Estimated 4-8 weeks for one engineer. Cap (also Tauri+Rust) moved its editor preview to a native GPU app for this reason: v0.6 reports CPU 76.6% -> 17.5%, memory 3.7 GiB -> 440 MiB, but it is an *experimental separate native app* (GPUI), not a layer inside the webview https://alternativeto.net/news/2026/9/cap-launches-v0-6-with-a-rebuilt-video-editor-3d-scenes-and-a-native-desktop-app/ . That is evidence that "all-native renderer" is the end state for this product class, and also evidence of its cost.
- Hit-testing/UX cost: overlays (manual-zoom marker, drag handles in `draw.js`) must sit above the native view; the web view must be transparent over the preview region only. Resize/scroll sync of two views is a classic source of jitter.

### 1b. Ship frames to the webview
Transports on macOS WKWebView (Tauri):
- Custom URI scheme (`register_asynchronous_uri_scheme_protocol`) + `fetch()`: binary, no base64. Anecdotal Tauri-Discord benchmark: ~10 MB in ~5 ms on macOS (non-scientific) https://github.com/orgs/tauri-apps/discussions/11915 . CSP caveat: a malformed CSP forces fallback to the slow postMessage IPC on macOS https://github.com/webdriverio/desktop-mobile/pull/552 (yours is `csp: null`, fine).
- Shared memory: not available for WKWebView; Tauri maintainers only discuss WebView2 SharedBuffer (same thread, #11915). Not an option.
- WebSocket binary (tokio-tungstenite on localhost): works, adds a dependency + port + security surface; no advantage over custom scheme for request/response frames.
- Throughput for 1280x720@60 (my arithmetic, UNVERIFIED on your machine): raw BGRA = 3.7 MB/frame = 221 MB/s: possible over a custom scheme (memcpy-bound) but wasteful and still needs `ImageData`/`createImageBitmap` upload each frame. JPEG q~0.75 at 720p screen content ~ 40-90 KB/frame = 2.4-5.4 MB/s at 60 fps: trivial; decode in WebKit via `createImageBitmap(blob)` is off-main-thread ImageIO. 60 fps preview is not needed anyway; 30 fps source frames suffice for a 60 Hz canvas with held frames.
- Conclusion: "ship JPEG frames over a custom scheme" is realistic and robust at 720p and well beyond.

Native decoders available for Rust (for sourcing frames):
| Option | Pros | Cons |
|---|---|---|
| AVAssetReader (objc2-av-foundation 0.3.2, already used in compress.rs) | zero new deps, HW decode, handles SCK HEVC/H.264 | sequential API; random access = new reader + `timeRange` per seek (tens of ms on long GOP), fine for generating a proxy, poor for scrubbing 2880x1800 long-GOP |
| VTDecompressionSession (objc2-video-toolbox 0.3.2) | per-sample random access, you control GOP walk | you must demux (symphonia/mp4) and build CMSampleBuffers/format descriptions yourself; more unsafe FFI |
| ffmpeg-next 9.0.0 | any format, sw/vt hwaccel | LGPL/GPL packaging, ~20-40 MB dylibs in .app, build complexity; unnecessary on macOS |
| screencapturekit 11.0.0 | capture only | not a decoder |
Pick AVAssetReader for proxy generation; VT only if you later want to scrub the *original* file.

## 2. Option B: keep web compositor, replace `<video>` with WebCodecs `VideoDecoder`

Facts found:
- WebCodecs exists in Safari/WebKit (MDN: VideoDecoder available in Dedicated Workers, secure context only; "not Baseline") https://developer.mozilla.org/en-US/docs/Web/API/VideoDecoder . You already depend on WebCodecs `VideoEncoder` in WKWebView for export, so the framework is present on your target.
- H.264 decode is the safe path. HEVC WebCodecs landed in WebKit mid-2023 https://lists.webkit.org/pipermail/webkit-changes/2023-July/288782.html but with reported bugs: `isConfigSupported` true then "Decoder failure" https://developer.apple.com/forums/thread/729464 ; frames not reordered by presentation time https://bugs.webkit.org/show_bug.cgi?id=270645 . Whether these are fixed in the WebKit shipping with macOS 26 is UNVERIFIED by me. Your SCK recordings may be HEVC -> risky; a proxy encoded as H.264 avoids it.
- `VideoDecoder` secure-context rule: custom `ipc://`/`asset://` origins in WKWebView: UNVERIFIED whether `window.isSecureContext` is true for Tauri's `tauri://localhost`. Check (`isSecureContext`) before committing; your VideoEncoder export presumably already works, which suggests yes.
- Pitfalls (general WebCodecs knowledge): must call `frame.close()` promptly (VideoFrames pin GPU memory; leak = freeze/jetsam), decoder queue/`decodeQueueSize` backpressure, need `description` (avcC) for AVC in `avc1` form, key-frame-first rule after `reset()`/seek, `flush()` latency, and no random access: you must walk from the previous keyframe (your 2880x1800 footage may have multi-second GOPs: 2880x1800 SW/HW decode of 60-120 frames per scrub is the same slowness as today).
- Drawing a `VideoFrame` with `drawImage` does not go through `MediaPlayerPrivateRemote`, so it **avoids the exact deadlock path**. This is its main attraction. Risk: it keeps decoding inside WebKit's GPU process (still one process sharing VT sessions), but without AVPlayer/rate/seek state machines.
- Demuxing: you need a demuxer in Rust or JS. `mp4` crate 0.14.0 is old (2023); `symphonia` 0.6.1 isomp4 is audio-oriented but exposes packets for video tracks (UNVERIFIED for H.264 `avcC` extraction ergonomics). Simpler: since you generate the proxy yourself, write your own tiny index (frame offsets/sizes/pts/is_key) at generation time and serve raw Annex-B/AVCC samples; no general demuxer needed. mp4box.js in JS is the other common route (not verified here).

Verdict: viable and the smallest JS change, but you inherit WebKit-version risk (HEVC), still need a proxy for fast random access, and still cannot give audio a stable master clock. As a *decoder* it is inferior to "JPEG strip" below for robustness; consider it only as phase-2 size optimization (H.264 proxy -> WebCodecs).

## 3. Proxy generation (low-res, short-GOP / all-intra)

Mechanics: AVAssetReader (BGRA or NV12 output, scaled via `AVVideoWidthKey`/`HeightKey` in the output settings or `VTPixelTransferSession`) -> either
 (i) AVAssetWriter with `AVVideoMaxKeyFrameIntervalKey = 1` (all-intra H.264/HEVC; Apple: `kVTCompressionPropertyKey_MaxKeyFrameInterval` default 0 lets the encoder choose; 1 = every frame key; encoders may still insert more) https://developer.apple.com/documentation/videotoolbox/encoding-video-for-live-streaming ; also set `AVVideoAllowFrameReorderingKey = false`, or
 (ii) per-frame JPEG (ImageIO `CGImageDestination` via objc2-image-io 0.3.2, or `turbojpeg` 1.5.1 / `jpeg-encoder` 0.7.1) packed into one `.sci` file: `[jpeg bytes...][u64 offset, u32 len, i64 pts_us]*N` footer, and a `memmap2` 0.9.11 reader.

Typical sizes (my estimates, UNVERIFIED; measure on your footage). Screen content is flat and compresses well:
| Proxy | Res/fps | Size per 10 min | Random access |
|---|---|---|---|
| All-intra H.264 (hw) | 1280x800 @30 | ~0.8-1.5 GB (~10-20 Mbps) | any frame, 1 decode |
| GOP=12 no-B H.264 | 1280x800 @30 | ~100-250 MB | <=11 extra decodes |
| JPEG strip q70 | 1280x800 @30 | ~0.6-1.1 GB (30-60 KB/frame) | none needed: byte slice |
| JPEG strip q70 | 960x600 @30 | ~0.35-0.7 GB | none needed |
Generation time: dominated by decode of 2880x1800 (VT hardware decodes this at several hundred fps on Apple Silicon, UNVERIFIED) and HW H.264 encode (compress.rs already hardware encodes); expect roughly 3-10x realtime for H.264 proxy, JPEG encode 3-6 ms/frame/core at 720p with turbojpeg (parallelize with rayon / 4 threads -> ~500+ fps) -> a 10 minute recording in ~20-60 s, generated in the background at recording stop so it is usually ready on editor open. Show a progress badge; fall back to current `<video>` path until ready.

Hybrid sizing recommendation: JPEG strip (simple, robust, zero decoder state) at 960 px wide for screen, 480 px for camera, 30 fps; keep to <= ~600 MB per hour-ish by lowering quality for long recordings; delete with the project cache. If disk size objectionable later, swap the strip's payload to H.264 GOP-12 decoded by VT in the same Rust API (the JS contract does not change).

Note: the preview shows the *zoomed* region at up to 2.0x+: a 960 px proxy of the whole screen will be soft when zoomed in at 100% preview. Mitigation: proxy width 1440 (2x of a 720p preview) for screens >= 2560 px, or serve a crop. Check `previewRes` max in `state.js`; this is the main quality trade-off of any proxy.

## 4. Rust audio engine as master clock

- Crates (verified current on crates.io 2026-10-12): `cpal 0.18.2` (CoreAudio output, callback thread), `rodio 0.23.0` (convenience on top; sink pause/seek is coarse and weaker for a sample-accurate clock), `symphonia 0.6.1` (already present: decode AAC/MP3/WAV/PCM), `rubato 5.0.1` (resample to device rate), `ringbuf 0.5.3` (lock-free SPSC for decoded PCM).
- Design: decode device audio/mic/music to f32 PCM in memory at project load (a 10 min stereo 48 kHz track = 230 MB f32; use 16-bit or mono mix for mic -> or stream-decode with a ring buffer; export already decodes whole files with `decodeAudioData`, so memory is already accepted). A mixer in the cpal callback sums tracks with gains, slice-aware timeline mapping (outToSrc: you already have `timemap.js`; mirror it in Rust or send the slice table down: `{out_start, src_start, rate, volume}`), and increments an `AtomicU64 samples_played`. **clock = samples_played / sr** is the output timeline in ms (minus `output_latency`, available from cpal `OutputCallbackInfo` timestamps).
- Speed ramps (`timeScale` 0.0625-16x): audio for non-1x slices must be resampled/time-stretched; today `playbackRate` on `<audio>` pitch-corrects. Simple approach: linear/`rubato` resample (changes pitch) or mute audio on non-1x slices (what many editors do); time-stretch (WSOLA/`signalsmith-stretch`: UNVERIFIED crate) is an optional later step.
- UI following the clock: do **not** push 60 events/s. JS keeps `tOut = anchorOut + (performance.now() - anchorPerf) * rate`; Rust pushes `{tOutMs, perfHintMs}` at ~4 Hz (or JS polls `await fetch("sc://clock")`/`invoke`) and JS slews the anchor (ignore error < 15 ms, jump if > 100 ms). During pause there is no clock at all. This gives drift-free A/V sync with no element state.
- WebAudio alternative (no Rust audio): decode to AudioBuffer in JS and play with `AudioBufferSourceNode`; `audioContext.currentTime` is a high-quality clock and WebAudio does not use AVPlayer, so it also avoids the deadlock. Cheaper to implement (1-2 days) than cpal, keeps the existing JS `masterGain/limiter` code (`audiolevel.js`). The Rust engine buys: independence from WebKit audio session quirks, background-safe, and a shared clock for a future native renderer. I rank WebAudio-clock as the **pragmatic first step**, Rust cpal as step 5.

## 5. Other relevant techniques and workarounds

- `requestVideoFrameCallback`: Safari 15.4+ supports it; it tells you when a new decoded frame is presented, so you only `drawImage` on new frames. It does not fix the sync-IPC freeze (drawImage on the element is still the call that blocks) and does not help seeking. Mild benefit; skip if `<video>` leaves the preview.
- `createImageBitmap(video)` / `new VideoFrame(video)`: still hits the same remote player readback path (UNVERIFIED but by construction). Not a workaround.
- OffscreenCanvas in a Worker (`ImageBitmapRenderer`/2D): moves *compositing* off the main thread so a stall does not freeze the UI. WKWebView supports OffscreenCanvas 2D since Safari 16.4 (UNVERIFIED for your target). Nice with the JPEG strip: worker does fetch -> createImageBitmap -> paintFrame -> transferToImageBitmap -> main `bitmaprenderer`. Requires `paintFrame` to be worker-safe (it uses `ctx` only, plus a cached static layer built from `document`? check `wallpapers.js`: uses `Image` elements: needs `createImageBitmap` in worker). Phase-4 item.
- WebKit bug history, relevant but none matching your exact stack (searched Bugzilla): GPUP deadlock while drawing MediaPlayer https://bugs.webkit.org/show_bug.cgi?id=243838 ; `-[AVPlayerItem currentTime]` hang in old Safari https://bugs.webkit.org/show_bug.cgi?id=147085 ; video-frame semaphore deadlocks https://bugs.webkit.org/show_bug.cgi?id=240161 , https://bugs.webkit.org/show_bug.cgi?id=236132 . I did not find an upstream fix for your trace; recommend filing a bug with the sample at bugs.webkit.org (low cost). Safari 18.2 release notes list seek/play ordering fixes only https://webkit.org/blog/16574/webkit-features-in-safari-18-2/ .
- Mitigations that reduce but do not eliminate the freeze (cheap, do now): never seek while another seek is pending (already done), never touch `playbackRate` during play (done), keep only ONE `<video>` with audio and drive camera as `<video muted>` only when needed, destroy/`removeAttribute("src")` hidden music `<audio>` elements until they become active (an idle AVPlayer is still a GPU-process object), avoid `preload="auto"` for elements outside the playhead range.
- Open-source references (Electron, not Tauri): OpenScreen and Capturia use a Swift ScreenCaptureKit helper + PixiJS preview https://pyshine.com/OpenScreen-Free-Screen-Recording-Studio/ ; Cap (Tauri/Rust) https://github.com/CapSoftware/Cap . I could not inspect their preview frame transport in this pass.

## 6. Comparison

| Criterion | A1 native layer | A2 Rust->JPEG->canvas | B WebCodecs+proxy | Status quo `<video>` |
|---|---|---|---|---|
| Removes AVPlayer deadlock | yes | yes | yes (no media elements) | no |
| Scrub latency | best | very good (byte slice + ImageIO decode ~3-8 ms) | good (GOP walk) | poor on long GOP |
| Keeps `paintFrame` = export parity | no (rewrite) | **yes** | yes | yes |
| Effort | 4-8 wks | 1.5-3 wks | 2-3 wks | 0 |
| Platform risk | high (unproven in Tauri) | low | medium (WebKit HEVC/H.264 bugs) | high |
| Disk | none | 0.3-1 GB/10 min (cache) | 0.1-1.5 GB | none |

## 7. RECOMMENDATION

**Rust-owned media, web-owned compositing**: keep `paintFrame()` and the 2D canvas (so preview and export remain the same code), but remove every `<video>`/`<audio>` from the preview path:
1. Rust generates a per-project **JPEG strip proxy** (screen 960-1440 px, camera 480 px, 30 fps) with AVAssetReader at recording stop / editor open.
2. Rust serves frame N via a custom URI scheme `sc://localhost/frame/{screen|camera}/{n}` (memmap2 slice, `Cache-Control: no-store`); JS prefetches +/- 6 frames into an `ImageBitmap` ring via `createImageBitmap` and draws from it.
3. Audio is played by WebAudio (step 2) then optionally a cpal engine; the audio clock is the single source of truth for `tOut` during play. No element is seeked or rate-changed during play, ever.
4. Export keeps the current full-res `<video>` seek path initially (it is sequential, un-contended, and works), then moves to a Rust AVAssetReader frame source (same custom scheme, `full` quality) to also eliminate its `seek()` cost.

Why this one: lowest risk of regression (render parity), no new native view/compositing, all decode state lives in Rust threads you control, scrubbing becomes O(1), and it is incrementally deliverable behind a flag with automatic fallback to `<video>` while the proxy builds. It does not depend on unverified WKWebView WebCodecs-HEVC behavior or on Tauri native-layer embedding that I could not find any shipping example of.

### Migration plan (ordered by risk/benefit)
| # | Step | Effort | Notes |
|---|---|---|---|
| 0 | Cheap mitigations (idle `<audio>` teardown, `preload=metadata`, single seek in flight) + file WebKit bug with the sample | 0.5 d | reduces freeze frequency only |
| 1 | `proxy.rs`: AVAssetReader -> scale -> turbojpeg -> `.sci` pack + index; command `build_proxy(dir, progress)`; background on record stop | 3-4 d | reuse compress.rs helpers |
| 2 | `sc://` scheme + `FrameSource` in JS (`ui/js/editor/frames.js`): `getFrame(kind, tMs)`, prefetch ring, close bitmaps | 2-3 d | S.proxy hook already in draw.js |
| 3 | Replace seek path: `requestSeek` -> `FrameSource` (no `seeked` wait); `draw()` uses bitmaps; keep `<video>` as fallback until proxy ready | 2 d | scrubbing win lands here |
| 4 | Playback loop driven by WebAudio clock; audio decoded to AudioBuffers; drop `<audio>`/`<video>` from playing path; slice timeScale via `playbackRate` on `AudioBufferSourceNode` (pitch changes, mute >4x) | 3-4 d | removes remaining deadlock surface |
| 5 | Rust cpal mixer + clock (optional) | 5-7 d | only if WebAudio quirks bite |
| 6 | Export via Rust frame source (full-res), drop `<video>` entirely | 3-5 d | |
| 7 | OffscreenCanvas worker compositing | 3-5 d | UI never blocks |
| 8 | (Optional) H.264 GOP proxy + VT decode to shrink disk | 4-6 d | same JS contract |
Total to "freeze-free": steps 0-4 ~ 2-3 weeks.

### Crates (versions verified on crates.io 2026-10-12)
`objc2 0.6.5`, `objc2-av-foundation 0.3.2` (already; add features `AVAssetReader*`, `AVVideoSettings` as needed), `objc2-core-video 0.3.2`, `objc2-core-media 0.3.2`, `objc2-image-io 0.3.2` (alt JPEG), `turbojpeg 1.5.1` (needs libjpeg-turbo built via cmake; or `jpeg-encoder 0.7.1` pure Rust, ~2-3x slower), `memmap2 0.9.11`, `rayon` (frame-parallel encode), `symphonia 0.6.1` (existing), optional `cpal 0.18.2`, `ringbuf 0.5.3`, `rubato 5.0.1`. No ffmpeg.

### Sketches

Proxy writer (Rust, `src-tauri/src/proxy.rs`):
```rust
// per frame: CMSampleBuffer -> CVPixelBuffer (BGRA, scaled by reader output settings) -> JPEG
let mut out = BufWriter::new(File::create(&tmp)?);
let mut index: Vec<(u64 /*off*/, u32 /*len*/, i64 /*pts_us*/)> = vec![];
while let Some(sb) = vout.copyNextSampleBuffer() {            // AVAssetReaderTrackOutput
    let pb = sb.image_buffer();                                 // CVPixelBuffer
    CVPixelBufferLockBaseAddress(pb, READONLY);
    let jpg = turbojpeg::compress(Image { pixels: base_slice, width, pitch: bytes_per_row,
                height, format: PixelFormat::BGRA }, 72, Subsamp::Sub2x2)?;  // use a worker pool for >1 core
    CVPixelBufferUnlockBaseAddress(pb, READONLY);
    index.push((out.stream_position()?, jpg.len() as u32, pts_us));
    out.write_all(&jpg)?;
    progress(pts_us as f64 / dur_us as f64);
}
write_footer(&mut out, &index)?; rename(tmp, final)?;           // atomic: partial files never served
```

Frame server (Rust, in `main.rs` builder):
```rust
.register_asynchronous_uri_scheme_protocol("sc", |_ctx, req, resp| {
    // path: /frame/screen/1234
    let (kind, n) = parse(req.uri().path());
    let bytes = STRIPS.get(kind).map(|s| s.frame(n)).unwrap_or(&[]); // &[u8] slice of Mmap
    resp.respond(Response::builder()
        .header("Content-Type", "image/jpeg")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(bytes.to_vec()).unwrap());   // copy ~50 KB; fine
});
```

JS frame source (`ui/js/editor/frames.js`):
```js
const ring = new Map();           // n -> Promise<ImageBitmap>
const FPS = 30;
export function frameIndex(kind, srcMs) { return Math.round(srcMs * FPS / 1000); }
function load(kind, n) {
  const k = kind + n;
  if (!ring.has(k)) ring.set(k, fetch(`sc://localhost/frame/${kind}/${n}`)
      .then(r => r.blob()).then(b => createImageBitmap(b)));
  return ring.get(k);
}
export async function getFrame(kind, srcMs, dir = 1) {
  const n = frameIndex(kind, srcMs);
  for (let i = 1; i <= 6; i++) load(kind, n + dir * i);      // prefetch
  const bmp = await load(kind, n);
  for (const [k, p] of ring) if (Math.abs(+k.replace(/\D/g, "") - n) > 24) { p.then(b => b.close()); ring.delete(k); }
  return bmp;
}
// draw.js: drawSrc.screen = lastBitmap (paintFrame already reads .width/.height when videoWidth is absent)
```
Note `paintFrame` already supports `v.videoWidth || v.width`, so an ImageBitmap works as-is, but the crop uses `vW`/`vH` of the *proxy*, which is normalized (`vx * vW`), so resolution-independent.

Clock (JS, WebAudio first):
```js
// start: all AudioBufferSourceNodes started at ctx.currentTime + 0.05 with offset
const startCtx = ctx.currentTime + 0.05, startOut = S.tOut;
function now() { return startOut + (ctx.currentTime - startCtx) * 1000; }   // output-timeline ms
function loop() { S.tOut = now(); seekFrames(S.tOut); draw(); if (S.playing) requestAnimationFrame(loop); }
```
Rust cpal clock (phase 5):
```rust
let played = Arc::new(AtomicU64::new(0));
let p = played.clone();
let stream = device.build_output_stream(&cfg, move |out: &mut [f32], info| {
    mixer.fill(out);                              // sums tracks, applies slice map + gains
    p.fetch_add((out.len() / ch) as u64, Ordering::Release);
}, err_fn, None)?;
// clock: tOutMs = played.load(Acquire) as f64 * 1000.0 / sr  - output_latency_ms
```

### Honest uncertainty
- Sizes, proxy encode speed, and `createImageBitmap` latency are estimates; do a 1-day spike (steps 1-2 for the screen track only) and measure on a 10 min 2880x1800 recording before committing.
- Zoomed preview sharpness vs proxy resolution is the main product trade-off; may require 1440-px proxy or a high-res "settle" frame fetched via `<video>`/AVAssetReader when paused (a single-frame Rust command `get_full_frame(t)` is easy and avoids long-lived players).
- I could not confirm any shipping Tauri app that layers a native video view under WKWebView, nor Cap's or Screen Studio's internal frame transport.
