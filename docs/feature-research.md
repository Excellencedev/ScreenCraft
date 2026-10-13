# Feature research: ideas from CapCut, Camtasia and Screen Studio (no AI)

Collected 2026-10-13 from vendor feature pages and comparison articles. Everything that
needs machine learning (auto-captions, background removal, motion tracking, text-to-speech,
AI voice effects) is deliberately left out. Vendor pages are marketing material, so
exact availability per version should be checked in the apps themselves.

## What the three tools offer

| Area | CapCut (desktop) | Camtasia | Screen Studio |
| --- | --- | --- | --- |
| Zoom / camera | Scale + keyframes, zoom-lens effects | Zoom-n-Pan animations, SmartFocus | Auto-zoom from clicks |
| Cursor | – | Scale, path editing, glow, replace pointer, click highlight, click sounds | Smoothing, styles, click effects |
| Annotations | Text, stickers | Callouts, arrows, shapes, spotlight, keystroke callouts, sketch motion | – |
| Animation | Keyframes on position / scale / rotation / opacity / colour; speed curves + presets | Animation library, motion paths, behaviours | – |
| Transitions | Large library (light leaks, 3D zooms) | 150+ | – |
| Speed | 0.1x–100x, curve mode with ramps, freeze frame | Clip speed | – |
| Masks / blend | Masks, blend modes, chroma key, HSL / curves colour | Chroma key, filters (30+) | – |
| Audio | Fades, volume keyframes | Levelling, normalisation, auto ducking, noise removal, visualisers, sound effects | Silence trimming |
| Captions | Auto (AI) + manual text | Manual / script / speech-to-text, closed captions | In-app transcript + captions |
| Camera layouts | Picture-in-picture layers | Picture-in-picture, templates | Camera shapes / positions |
| Export | One-step export, up to 4K | Presets, batch, share | 4K60 |

Sources: [CapCut keyframe tool](https://www.capcut.com/tools/keyframe-animation),
[CapCut transitions](https://www.capcut.com/tools/free-video-transitions),
[CapCut desktop overview](https://www.capcut.com/resource/pc-professional-video-editor),
[Camtasia features](https://www.techsmith.com/camtasia/features/),
[Camtasia vs Screen Studio](https://trainn.co/blog/camtasia-vs-screen-studio/),
[CapCut zoom for tutorials](https://www.capcut.com/create/zoom-effects-for-tutorial-videos-guide-attention).

## What ScreenCraft already has

Auto-zoom with manual regions, cursor smoothing / styles / click effects / hide while
typing, camera bubble with border and shrink-on-zoom, backgrounds / padding / corners /
shadow, per-clip speed, split / trim / reorder, music with waveform, mic levelling, click
and typing sound effects, presets, timeline zoom, MP4 / GIF export with compression.

## Recommended next features, ranked for a screen-recording editor

Rank = how often a tutorial / demo creator would use it x how well it fits our design.

| # | Feature | Seen in | Fit | Effort | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | **Text and callouts**: titles, arrows, boxes, with fade / pop animation, placed on the timeline | CapCut, Camtasia | Very high | M | New "Annotations" track; render in the shared compositor so export matches. |
| 2 | **Blur / pixelate / spotlight regions** (hide secrets, dim everything but one area) | Camtasia, CapCut masks | Very high | M | Same region machinery as zoom; blur via downscale-upscale (already used for backgrounds). |
| 3 | **Keystroke overlay**: show pressed shortcuts as a pill | Camtasia | Very high | S | We already log every key and modifier timing; needs modifier capture + a renderer. |
| 4 | **Speed up idle / silent parts** automatically (no AI: input activity + audio level) | Screen Studio (silence trim) | High | M | Builds on slice speed; propose segments, user accepts. |
| 5 | **Speed ramps**: ease into / out of fast-forward | CapCut curves | High | M | Needs a time-warp inside a slice; timeline mapping is already isolated in core. |
| 6 | **Transitions** between clips (fade, dissolve, wipe, push) | CapCut, Camtasia | High | M | Overlap two sources in the compositor; start with fade and cross-zoom. |
| 7 | **Captions from SRT / typed subtitles** with styles, burn-in | Camtasia, CapCut | High | M | Manual only. Import/export SRT; text track reuses #1. |
| 8 | **Audio fades and music ducking** under speech | CapCut, Camtasia | High | S | Fade handles on music clips; side-chain from mic level on the offline mix. |
| 9 | **Freeze frame / hold** | CapCut | Medium | S | A one-frame slice with a duration. |
| 10 | **Camera layouts**: camera-only, side-by-side, big/small per clip | Screen Studio, Camtasia | Medium | M | Per-slice layout enum read by the compositor. |
| 11 | **Keyframes** for camera bubble and overlays (position, scale, opacity) | CapCut, Camtasia | Medium | L | Generic property curves; also unlocks animated annotations. |
| 12 | **Intro / outro title cards, watermark / logo overlay** | CapCut, Camtasia | Medium | S | Static layers on the first / last seconds. |
| 13 | **Camera chroma key** (green screen) | CapCut, Camtasia | Medium | S | Per-pixel key in the worker; no ML. |
| 14 | **Colour adjustments** (brightness, contrast, saturation, LUT) | CapCut, Camtasia | Low | S | Canvas filter on the screen layer; export parity needs care. |
| 15 | **Export presets for platforms** (YouTube, X, vertical 9:16 follow-cursor, GIF loop) | CapCut | Medium | S | Presets over existing dialog values. |
| 16 | **Device frames / browser chrome mockups** | – | Medium | S | Drawn around the content rect. |
| 17 | **Brand kits**: shareable preset files (colours, logo, cursor) | Camtasia | Low | S | Export / import the existing preset JSON. |

Left out on purpose: auto-captions, background removal, motion tracking, text-to-speech
and anything else that needs a model; cloud collaboration and stock libraries (licensing
and hosting, not a fit for an offline local-first app).

## Status (2026-10-13)

Built: text and callouts (1), blur / pixelate / spotlight (2), keystroke overlay (3),
idle speed-up (4), speed ramps (5), cut transitions (6, as dip-style effects on a single
lane: fade, flash, blur, slide, zoom), freeze frame (9), music fades and ducking (8) and
camera layouts (10).

Still open: subtitles (7), true cross-dissolve (needs overlapping clips), keyframes (11),
title cards and logo (12), chroma key (13), colour adjustments (14), platform export
presets (15), device frames (16), brand kits (17).
