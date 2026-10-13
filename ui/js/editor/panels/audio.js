import { P, media } from "../state.js";
import { h3, el, slider, toggle, segmented } from "../widgets.js";
import { hooks } from "../state.js";
import { previewSound } from "../audio.js";
import { CLICK_STYLES, KEY_STYLES } from "../../sounds.js";
import { refreshSliceAudio } from "../playback.js";
import { addMusic, removeMusicClip, clips } from "../music.js";
import { S } from "../state.js";

export function panelAudio(p) {
  const a = P().config.audio;
  const m = P().media;
  p.append(h3("Audio"));
  if (!m.mic && !m.hasMicrophone && !m.hasSystemAudio) {
    p.append(el('<div class="muted">No audio was recorded. Enable the microphone or system audio in the recorder toolbar.</div>'));
  }
  const apply = () => {
    refreshSliceAudio();
  };
  p.append(slider("Master volume", 0, 1, 0.01, () => a.volume, (v) => { a.volume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  p.append(slider("Microphone", 0, 1, 0.01, () => a.micVolume, (v) => { a.micVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  if (!m.mic) {
    p.append(el('<div class="hint muted">This recording has no separate microphone track (it was recorded before microphones were split out), so the Device slider controls everything.</div>'));
  }
  p.append(slider("Device audio", 0, 1, 0.01, () => a.deviceVolume, (v) => { a.deviceVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  p.append(h3("Sound effects"));
  const resync = () => { hooks.syncMusic?.(); };
  p.append(toggle("Mouse click sounds", "Plays a click at every click in the recording.", () => a.clickSounds, (v) => { a.clickSounds = v; resync(); }));
  if (a.clickSounds) {
    p.append(segmented("Click style", CLICK_STYLES, () => a.clickStyle, (v) => { a.clickStyle = v; resync(); previewSound("click", v); }));
    p.append(slider("Click volume", 0, 1, 0.01, () => a.clickVolume ?? 0.6, (v) => { a.clickVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  }
  p.append(toggle("Keyboard typing sounds", "Plays a key sound for every key you pressed.", () => a.keySounds, (v) => { a.keySounds = v; resync(); }));
  if (a.keySounds) {
    p.append(segmented("Keyboard style", KEY_STYLES, () => a.keyStyle, (v) => { a.keyStyle = v; resync(); previewSound("key", v); }));
    p.append(slider("Typing volume", 0, 1, 0.01, () => a.keyVolume ?? 0.5, (v) => { a.keyVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
  }
  p.append(h3("Music"));
  const list = clips();
  if (list.length) {
    p.append(slider("Music volume", 0, 1, 0.01, () => a.musicVolume ?? 0.5, (v) => { a.musicVolume = v; apply(); }, { fmt: (v) => Math.round(v * 100) + "%", track: false }));
    p.append(el('<div class="hint muted">Drag a green block to place it, drag its ends to trim, and press S to split at the playhead.</div>'));
  }
  const add = el('<button class="btn-line">Add music…</button>');
  add.onclick = addMusic;
  p.append(add);
  if (S.selected?.type === "music") {
    const rm = el('<button class="btn-line danger">Delete selected music clip</button>');
    rm.onclick = () => removeMusicClip(S.selected.id);
    p.append(rm);
  }
}
