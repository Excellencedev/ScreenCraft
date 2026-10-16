# native-egui: status and open items

This branch rebuilds ScreenCraft as a native egui/wgpu app that matches the 0.3 (Tauri) UI:
the same colours, layout, panels, timeline, recorder bar, dialogs and features, with no web view.
Do not merge into `main` until the owner says so.

## Done
- Editor: top bar, icon rail with nine panels, six-track timeline, transport, export, presets,
  raw files, about / updates (all drawn to the 0.3 CSS).
- Compositor: camera layouts and bubble, device frames, annotations (text, arrow, box, circle,
  highlight, blur, pixelate, spotlight), shortcut display, captions, watermark, cross-dissolve and
  cut transitions, background image / blur / motion, motion blur on export.
- Audio: music with fades and ducking, click and typing sounds, master / mic levelling.
- Captions with whisper.cpp, SRT import / export, GIF export, the compressor (standalone + after export).
- Recorder bar (modes, inputs, quality, projects), countdown, HUD, area picker, permissions window,
  camera capture, update check.
- H.264 through the operating system only (VideoToolbox on macOS, Media Foundation on Windows).
  OpenH264 is gone.

## Known gaps / not verified
- Windows: `crates/encode/src/mf.rs` (Media Foundation H.264) was written without a Windows
  machine and has not been compiled or run. Capture backends for Windows / Linux were only
  type-checked earlier.
- Linux: there is no operating-system H.264, so recordings cannot be edited there yet (record in
  AV1 / VP9 and decode with a bundled decoder is the open design question).
- Camera recording, microphone meter and whisper were built but not exercised end to end (they
  need device permissions / a model download).
- HEVC export and compression (0.3 offered it): H.264 only for now.
- Compressor reads H.264 MP4s with Opus audio; AAC audio in an input is left out.
- Menu-bar tray icon, Bluetooth-headphone audio guard, HUD placement next to the area, live
  system-audio meter before recording.
- 0.3 recordings: camera.mp4 (fragmented MP4) and mic.m4a are read; AAC in screen.mp4 is not.
