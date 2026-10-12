# ScreenCraft

A native-feeling macOS screen recorder and editor in the spirit of Screen Studio,
built in Rust. Record your display, a window, or an area (plus camera, microphone
and system audio), then polish it in the editor: automatic zooms that follow your
clicks, spring-smoothed cursor, backgrounds, padding, rounded corners, shadows,
camera bubble, trimming/cutting, speed changes and MP4/GIF export.

No AI features — just recording, camera, editing and zoom.

## Compression (HandBrake-style)

The toolbar's compress button (or the menu-bar item *Compress Video…*) opens a
compressor for any video file: presets (Fast 1080p30, HQ, Super HQ, 720p, 480p,
HEVC "tiny"), codec (H.264 / H.265), resolution, frame-rate cap, quality slider,
audio bitrate or removal, and a live size estimate. It runs natively in Rust on
AVFoundation's hardware encoders. The editor's export dialog also offers codec
and quality choices.

## Architecture

| Piece | Where | What it does |
| --- | --- | --- |
| `screencraft-core` | `crates/core` | Pure Rust: project model, spring physics, timeline mapping, auto-zoom generation, per-frame camera + cursor render track. Unit tested. |
| App shell | `src-tauri` | Tauri 2 app. ScreenCaptureKit capture (`SCRecordingOutput`), global cursor/click tracker, project IO, window management. |
| UI | `ui` | Recorder toolbar, area picker and editor (canvas compositor + timeline). Paints the track computed in Rust; exports with WebCodecs. |

Capture writes a `.screencraft` package under `~/ScreenCraft Projects/`:
`screen.mp4`, optional `camera.mp4`, `input.json` (cursor moves, clicks) and `project.json`.
The system cursor is *not* baked into the video, so the editor can redraw, smooth and
restyle it.

## Build & run

Requirements: macOS 15+, Rust (stable), Xcode command line tools.

```bash
cargo test -p screencraft-core        # core unit tests
./scripts/bundle.sh                   # release build -> dist/ScreenCraft.app
open dist/ScreenCraft.app
```

The first recording asks for **Screen Recording** permission (System Settings →
Privacy & Security). Camera and microphone prompts appear when you enable them.

Global shortcut: **⌘⇧2** toggles recording. The menu-bar icon has *New Recording* / *Stop*.

## Editor shortcuts

`Space` play/pause · `S` split at playhead · `Z` add zoom · `⌫` delete selection ·
`←/→` step · `⌘Z / ⇧⌘Z` undo/redo · `⌘E` export · `⌘S` save.

## Notes

- Exports use WebCodecs (H.264/AAC) inside the web view and a bundled MP4 muxer
  (`ui/vendor`, MIT) — no ffmpeg required.
- Wallpapers are generated procedurally; no third-party artwork is bundled.
- Behaviour was studied by observing a licensed copy of Screen Studio; no code or
  assets were copied.
