import { P, media } from "../state.js";
import { h3, el, slider } from "../widgets.js";
import { refreshSliceAudio } from "../playback.js";
import { addMusic, removeMusic } from "../music.js";

export function panelAudio(p) {
  const a = P().config.audio;
  const m = P().media;
  p.append(h3("Audio"));
  if (!m.mic && !m.hasMicrophone && !m.hasSystemAudio) {
    p.append(el('<div class="muted">No audio was recorded. Enable the microphone or system audio in the recorder toolbar.</div>'));
  }
  const apply = () => {
    if (media.screen) refreshSliceAudio();
  };
  p.append(slider("Master volume", 0, 1, 0.01, () => a.volume, (v) => { a.volume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  p.append(slider("Microphone", 0, 1, 0.01, () => a.micVolume, (v) => { a.micVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  if (!m.mic) {
    p.append(el('<div class="hint muted">This recording has no separate microphone track (it was recorded before microphones were split out), so the Device slider controls everything.</div>'));
  }
  p.append(slider("Device audio", 0, 1, 0.01, () => a.deviceVolume, (v) => { a.deviceVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  p.append(h3("Music"));
  if (P().scene.music) {
    p.append(el(`<div class="hint muted">♪ ${P().scene.music.name} — drag the green block on the timeline to place it.</div>`));
    p.append(slider("Music volume", 0, 1, 0.01, () => a.musicVolume ?? 0.5, (v) => { a.musicVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
    const rm = el('<button class="btn-line danger">Remove music</button>');
    rm.onclick = removeMusic;
    p.append(rm);
  } else {
    const add = el('<button class="btn-line">Add music…</button>');
    add.onclick = addMusic;
    p.append(add);
  }
}
