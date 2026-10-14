# Changelog

## Unreleased

- **Bluetooth headphones stay in stereo while you record.** Opening a Bluetooth headset's microphone
  makes the headphones drop to call quality (16 kHz mono), which ruined what you heard and what was
  recorded. ScreenCraft now prefers the Mac's own microphone, never opens a headset microphone by
  accident (not even to fill the device list), and while a microphone is in use it pins the system's
  default input to the built-in one, restoring it when you turn the microphone off or quit. Picking
  a Bluetooth microphone on purpose still works and says what it costs.

## 0.3.0

- **Auto captions:** *Captions → Generate captions* transcribes the microphone on your machine with
  [whisper.cpp](https://github.com/ggml-org/whisper.cpp). Pick a model (tiny / base / small English, or
  multilingual base); it is downloaded once on request. Nothing is uploaded.
- **Device frames:** laptop, monitor, tablet, phone and browser window around the screen.
- **Animated wallpapers:** subtle motion (drift, bokeh, waves, dust) on any wallpaper, identical in preview and export.
- Legal and compliance: `THIRD-PARTY-NOTICES.md`, `COMPLIANCE.md`, `SECURITY.md`, license/disclaimer in the README.
- A website (Vite, in `website/`) with docs, downloads and legal pages, deployed on Cloudflare.

## 0.2.0

- Sped-up clips show one compact pill ("2×"), and a speed ramp is one block ("2.3× → 4.2×")
  instead of a row of slivers; zoom regions over a ramp stay one block too.
- Watermark: upload a logo, place it on a 3×3 grid, set size and opacity (also in exports).
- Zoom regions always follow the cursor (the manual option is gone).
- New shortcuts: Q / E jump to the start / end, A / D jump to the previous / next zoom start.
  Duplicate moved to ⌘D.

- The cursor now hides when you start typing and stays hidden until the mouse moves again
  (shortcuts like ⌘C do not hide it).

**Editor**
- Annotations: text, arrows, boxes, circles, highlights, blur, pixelate and spotlight,
  pinned to the recording or fixed on the frame, dragged on the preview.
- Captions: type or import SRT, TikTok-style default (white text, heavy black outline, middle
  of the bottom third) plus TikTok red, highlight, classic and minimal presets; export SRT.
- Shortcut display: pops up recorded shortcuts and special keys (only those are stored).
- Camera layouts track: large bubble, camera only, side by side (either side), camera off.
  Every block can have its own camera settings (size, position, shape, mirror, border, shadow,
  flush-to-edge) and the camera eases smoothly between blocks. Nine anchor points for the
  bubble and a flush-to-edge option.
- Pace: speed up idle stretches, speed ramps, freeze frame and cut transitions (cross-dissolve,
  fade, flash, blur, slide, zoom).
- Music fades and ducking under speech; built-in click and typing sound effects.
- Hold Space for 2x; timeline zoom; multi-select zoom regions (shift-click) that edit and move
  together; draggable timeline and side-panel dividers; Esc closes dialogs; presets keep 2%
  padding and can be renamed.

**Recorder**
- Spinners while inputs load and Record stays disabled until ready; cancel the countdown.
- The preview proxy builds in the background as soon as a recording stops; long recordings get
  a lighter proxy.

**Export**
- Roughly 40-65 fps at 1080p60 with source frames scaled to what each frame needs.
- No more silent hangs: if the hardware video encoder stops answering (the export used to sit
  at "Rendering frame 13" forever), the export now notices, retries once with the software
  encoder, and otherwise shows an error. Cancel works while it waits, and reading a frame
  from the recording times out instead of waiting forever.

**Platform**
- Windows support (thanks @PierrunoYT) and an NSIS installer script.
- New app icon, DMG and installer scripts.

## 0.1.0

First public version: recording, editing, zoom, export and compression on macOS.
