# Long-recording test (2026-10-13)

One real recording through the whole pipeline on an M-series MacBook Air (16 GB):
11 min 17 s, native 2880x1800 at 60 fps, camera + microphone + system audio.

| Stage | Result |
| --- | --- |
| Recording | App memory plateaued at ~1.35 GB (screen is written by ScreenCaptureKit, not by the app); CPU ~6%; ~1.6 MB/s to disk. |
| Stop -> editor | Project saved immediately; the preview proxy built in the background in ~2.5 min and was ready when the editor opened (no "Preparing preview" wait). Peak ~1.5 GB. |
| Editor | Scrubbing to arbitrary points was instant; playback clock matched wall time (10 s -> 10 s); camera was not frozen anywhere. |
| Export | 1080p60 H.264 + AAC: 40,621 frames in 628 s (64.7 fps, ~real time). WebKit peak ~2.2 GB (the MP4 is muxed in memory). Output 677.0 s, 466 MB. |

## Fixed from this pass

- The preview proxy was 2.46 GB for the screen (5x the source video). Recordings over 4 min
  now use lighter tiers: <=4 min 1280 px q62, <=8 min 1120 px q56, longer 960 px q50.
  The same recording went from 2.8 GB to 1.15 GB of proxy files with the same build time.
  The paused frame is still fetched at full resolution.

## Known limits

- Export muxes in memory, so very long exports (30+ min) will need several GB of RAM.
- A native 60 fps recording is ~1.6 MB/s plus a proxy; a one-hour recording needs ~10 GB free.
