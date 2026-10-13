import { S, P, hooks } from "../state.js";
import { h3, el, slider, toggle, segmented } from "../widgets.js";
import { findIdle, applyIdle, easeSpeedChanges, freezeAtPlayhead } from "../pace.js";

// remembered across panel redraws
const opt = { minIdle: 2.5, speed: 4, keepSpeech: true, ramp: 0.4, hold: 1.5, status: "" };

const KINDS = [
  [null, "None"], ["dissolve", "Dissolve"], ["fade", "Fade"], ["flash", "Flash"], ["blur", "Blur"], ["slide", "Slide"], ["zoom", "Zoom"],
];

export function panelPace(p) {
  p.append(h3("Speed up idle parts"));
  p.append(el('<div class="muted">Finds stretches with no clicks, typing or cursor movement (and no talking) and fast-forwards them.</div>'));
  p.append(slider("Idle for at least", 1, 10, 0.5, () => opt.minIdle, (v) => (opt.minIdle = v), { fmt: (v) => v.toFixed(1) + " s", track: false, key: "pace-min" }));
  p.append(segmented("Speed", [[2, "2×"], [4, "4×"], [8, "8×"]], () => opt.speed, (v) => (opt.speed = v), { track: false }));
  if (P().media.mic) {
    p.append(toggle("Keep the parts where I'm talking", null, () => opt.keepSpeech, (v) => (opt.keepSpeech = v), { track: false }));
  }
  const go = el('<button class="btn-line">Find and speed up</button>');
  const status = el(`<div class="hint muted">${opt.status}</div>`);
  go.onclick = async () => {
    go.disabled = true;
    go.textContent = "Looking…";
    try {
      const segs = await findIdle(opt.minIdle * 1000, opt.keepSpeech && !!P().media.mic);
      if (!segs.length) {
        opt.status = "No idle stretches that long. Try a shorter minimum.";
      } else {
        const r = applyIdle(segs, opt.speed);
        const eased = opt.ramp > 0 ? easeSpeedChanges(opt.ramp * 1000) : 0;
        opt.status = `Sped up ${r.count} stretch${r.count === 1 ? "" : "es"}${eased ? ", eased in and out" : ""}. Saved ${(r.savedMs / 1000).toFixed(1)} s.`;
      }
    } catch (e) {
      opt.status = "Could not analyse the recording: " + (e?.message || e);
    }
    hooks.renderPanel();
  };
  p.append(go, status);

  p.append(h3("Speed ramps"));
  p.append(el('<div class="muted">Turns each abrupt change of speed between clips into a smooth ease.</div>'));
  p.append(slider("Ease length", 0.2, 1.5, 0.1, () => opt.ramp, (v) => (opt.ramp = v), { fmt: (v) => v.toFixed(1) + " s", track: false, key: "pace-ramp" }));
  const ease = el('<button class="btn-line">Ease all speed changes</button>');
  ease.onclick = () => {
    const n = easeSpeedChanges(opt.ramp * 1000);
    opt.status = n ? `Eased ${n} speed change${n === 1 ? "" : "s"}.` : "No speed changes to ease (or the clips are too short).";
    hooks.renderPanel();
  };
  p.append(ease);

  p.append(h3("Freeze frame"));
  p.append(slider("Hold for", 0.5, 5, 0.5, () => opt.hold, (v) => (opt.hold = v), { fmt: (v) => v.toFixed(1) + " s", track: false, key: "pace-hold" }));
  const fz = el('<button class="btn-line">Freeze at playhead</button>');
  fz.onclick = () => freezeAtPlayhead(opt.hold * 1000);
  p.append(fz);

  p.append(h3("Transition"));
  const slices = P().scene.slices;
  const idx = S.selected?.type === "slice" ? slices.findIndex((s) => s.id === S.selected.id) : -1;
  if (idx <= 0) {
    p.append(el('<div class="muted">Click the + on the timeline at a cut between two clips to give it a transition.</div>'));
    return;
  }
  const s = slices[idx];
  p.append(el(`<div class="hint muted">At the cut before “${s.freeze ? "freeze" : "clip " + (idx + 1)}”.</div>`));
  p.append(segmented("Style", KINDS.map(([k, n]) => [k ?? "none", n]), () => s.transition?.kind ?? "none", (v) => {
    s.transition = v === "none" ? null : { kind: v, durationMs: s.transition?.durationMs ?? 500 };
  }, { track: true, timeline: true }));
  if (s.transition) {
    p.append(slider("Length", 0.2, 1.5, 0.1, () => s.transition.durationMs / 1000, (v) => (s.transition.durationMs = Math.round(v * 1000)), { fmt: (v) => v.toFixed(1) + " s", track: true, key: "trans-len-" + s.id }));
    p.append(el('<div class="hint muted">Dissolve blends the two clips into each other. Fade dips to black, Flash to white, Blur softens the cut, Slide pushes the picture away and Zoom punches in.</div>'));
  }
}
