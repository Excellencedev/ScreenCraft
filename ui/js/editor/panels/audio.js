import { P, media } from "../state.js";
import { h3, el, slider } from "../widgets.js";

export function panelAudio(p) {
  const a = P().config.audio;
  p.append(h3("Audio"));
  if (!P().media.hasMicrophone && !P().media.hasSystemAudio) {
    p.append(el('<div class="muted">No audio was recorded. Enable the microphone or system audio in the recorder toolbar.</div>'));
  }
  p.append(slider("Volume", 0, 1, 0.01, () => a.volume, (v) => {
    a.volume = v;
    if (media.screen) media.screen.volume = Math.min(1, Math.max(0, v));
  }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
}
