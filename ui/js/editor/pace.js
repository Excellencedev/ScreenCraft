// Pace tools: idle fast-forward, speed ramps, freeze frames and transitions.
// All of them rewrite the clip list (no new timeline model), so playback, audio and export
// keep working from the same slices. Everything goes through change() and is undoable.
import { invoke } from "../tauri.js";
import { S, P, dir, uid } from "./state.js";
import { change } from "./history.js";
import { outToSrc, sliceLen } from "./timemap.js";

/** Ask the backend for idle stretches (source ms). */
export async function findIdle(minIdleMs, keepSpeech) {
  return invoke("find_idle", { dir, project: P(), minIdleMs, keepSpeech });
}

/**
 * Fast-forward the given source ranges. Splits clips at the range edges and multiplies the
 * speed of the pieces inside. Returns { count, savedMs }.
 */
export function applyIdle(segments, speed) {
  let count = 0;
  const before = P().scene.slices.reduce((a, s) => a + sliceLen(s), 0);
  change(null, () => {
    let slices = P().scene.slices;
    for (const [a, b] of segments) {
      const next = [];
      for (const s of slices) {
        if (s.freeze || s.sourceEndMs <= a || s.sourceStartMs >= b) {
          next.push(s);
          continue;
        }
        const lo = Math.max(a, s.sourceStartMs), hi = Math.min(b, s.sourceEndMs);
        if (hi - lo < 250) {
          next.push(s);
          continue;
        }
        count++;
        if (lo > s.sourceStartMs + 1) next.push({ ...s, id: uid(), sourceEndMs: lo });
        next.push({
          ...s, id: uid(), sourceStartMs: lo, sourceEndMs: hi, timeScale: +(s.timeScale * speed).toFixed(3),
          transition: lo > s.sourceStartMs + 1 ? null : s.transition,
        });
        if (hi < s.sourceEndMs - 1) next.push({ ...s, id: uid(), sourceStartMs: hi, transition: null });
      }
      slices = next;
    }
    P().scene.slices = slices;
    S.selected = null;
  }, { panel: true, timeline: true });
  const after = P().scene.slices.reduce((a, s) => a + sliceLen(s), 0);
  return { count, savedMs: Math.max(0, before - after) };
}

/**
 * Replace each abrupt change of speed between neighbouring clips by a short staircase of
 * intermediate speeds (about `rampMs` of output time), so fast-forward eases in and out.
 * `onlyIndex`, when given, limits it to the cut at the start of that clip.
 */
export function easeSpeedChanges(rampMs, onlyIndex = null) {
  let made = 0;
  change(null, () => {
    const out = P().scene.slices.slice();
    // from the end, so earlier indexes stay valid while ramps are spliced in
    for (let i = out.length - 2; i >= 0; i--) {
      if (onlyIndex != null && i + 1 !== onlyIndex) continue;
      const A = out[i], B = out[i + 1];
      if (A.freeze || B.freeze) continue;
      const a = A.timeScale, b = B.timeScale;
      if (Math.abs(Math.log(b / a)) < 0.25) continue;
      const N = 8;
      const ks = [];
      for (let j = 0; j < N; j++) ks.push(a * Math.pow(b / a, (j + 0.5) / N));
      // source time the ramp consumes from each side, for a given ramp length in output ms
      const cost = (R) => {
        const per = R / N;
        let sa = 0, sb = 0;
        ks.forEach((k, j) => (j < N / 2 ? (sa += k * per) : (sb += k * per)));
        return [sa, sb];
      };
      let R = rampMs;
      let [sa, sb] = cost(R);
      const lenA = A.sourceEndMs - A.sourceStartMs, lenB = B.sourceEndMs - B.sourceStartMs;
      const f = Math.min(1, (lenA * 0.45) / sa, (lenB * 0.45) / sb);
      if (f < 0.25) continue; // the clips are too short to ease between
      R *= f;
      [sa, sb] = cost(R);
      const per = R / N;
      // the first half of the staircase eats the end of A, the second half the start of B
      let curA = A.sourceEndMs - sa, curB = B.sourceStartMs;
      const ramp = ks.map((k, j) => {
        const len = k * per;
        const fromA = j < N / 2;
        const start = fromA ? curA : curB;
        const piece = {
          ...(fromA ? A : B), id: uid(), sourceStartMs: start, sourceEndMs: start + len,
          timeScale: +k.toFixed(3), transition: null, freeze: false,
        };
        if (fromA) curA += len;
        else curB += len;
        return piece;
      });
      A.sourceEndMs -= sa;
      B.sourceStartMs += sb;
      out.splice(i + 1, 0, ...ramp);
      made++;
    }
    P().scene.slices = out;
  }, { panel: true, timeline: true });
  return made;
}

/** Hold the frame under the playhead for `holdMs`. */
export function freezeAtPlayhead(holdMs) {
  const { src, idx } = outToSrc(S.tOut);
  const sl = P().scene.slices[idx];
  if (!sl || sl.freeze) return false;
  change(null, () => {
    const slices = P().scene.slices;
    const at = Math.min(Math.max(src, sl.sourceStartMs + 5), sl.sourceEndMs - 5);
    const frozen = {
      ...sl, id: uid(), freeze: true, transition: null, volume: 0,
      sourceStartMs: at, sourceEndMs: at + holdMs / 1000, timeScale: 0.001,
    };
    const right = { ...sl, id: uid(), sourceStartMs: at, transition: null };
    sl.sourceEndMs = at;
    slices.splice(idx + 1, 0, frozen, right);
    S.selected = { type: "slice", id: frozen.id };
  }, { panel: true, timeline: true });
  return true;
}

/** Set (or clear, with kind = null) the transition at the start of slice `id`. */
export function setTransition(id, kind, durationMs = 500) {
  const s = P().scene.slices.find((x) => x.id === id);
  if (!s) return;
  change("trans-" + id, () => {
    s.transition = kind ? { kind, durationMs } : null;
  }, { panel: true, timeline: true });
}
