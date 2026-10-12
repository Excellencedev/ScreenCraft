// Shared editor state. Modules import `S` and talk to each other via `hooks`
// (registered in main.js) so there are no circular imports.
export const $ = (id) => document.getElementById(id);
export const dir = new URLSearchParams(location.search).get("dir");
export const TRACK_FPS = 60;

export const S = {
  project: null,
  track: null,
  tOut: 0,
  playing: false,
  scrubbing: false,
  dirty: false, // edits not yet saved
  proxy: null, // in-memory scrub frames while dragging the playhead
  selected: null, // {type:'zoom'|'slice', id}
  tab: "background",
  previewRes: 720,
  history: [],
  future: [],
  lastKey: null,
  lastT: 0,
  curSlice: 0,
};

/** Decoded media elements and images used by the compositor. */
export const media = { screen: null, camera: null, mic: null, musicEls: {}, bgImage: null, micLevel: 1, micBaked: false };

export const P = () => S.project;
export const uid = () => Math.random().toString(36).slice(2, 10);
export const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
export function fmt(ms) {
  const s = Math.max(0, ms) / 1000;
  return `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, "0")}`;
}

export const hooks = {
  draw() {},
  renderTimeline() {},
  renderPanel() {},
  scheduleTrack() {},
  syncControls() {},
  seek(_t) {},
};
