// Output (edited) time <-> source (recorded) time. Mirrors timeline.rs.
import { P, clamp } from "./state.js";

export const sliceLen = (s) => Math.max(0, s.sourceEndMs - s.sourceStartMs) / Math.max(1e-3, s.timeScale);
export const outDuration = () => P().scene.slices.reduce((a, s) => a + sliceLen(s), 0);

export function outToSrc(t) {
  let acc = 0;
  const sl = P().scene.slices;
  for (let i = 0; i < sl.length; i++) {
    const len = sliceLen(sl[i]);
    if (t < acc + len || i === sl.length - 1) {
      return { src: clamp(sl[i].sourceStartMs + (t - acc) * sl[i].timeScale, sl[i].sourceStartMs, sl[i].sourceEndMs), idx: i };
    }
    acc += len;
  }
  return { src: 0, idx: 0 };
}

/** Output start time of slice `idx`. */
export function sliceOutStart(idx) {
  let acc = 0;
  const sl = P().scene.slices;
  for (let i = 0; i < idx && i < sl.length; i++) acc += sliceLen(sl[i]);
  return acc;
}

/** Output time of a source time (first slice that contains it), or null if it was cut. */
export function srcToOut(t) {
  let acc = 0;
  for (const s of P().scene.slices) {
    if (t >= s.sourceStartMs && t <= s.sourceEndMs) return acc + (t - s.sourceStartMs) / s.timeScale;
    acc += sliceLen(s);
  }
  return null;
}

/**
 * Every stretch of the timeline that shows source range [a, b], as
 * [outStart, outEnd, srcStart, srcEnd]. Pieces that are back-to-back both on the
 * timeline and in the source (e.g. after a split) are merged into one stretch.
 */
export function spansOut(a, b) {
  const out = [];
  let acc = 0;
  let prev = null;
  for (const s of P().scene.slices) {
    const lo = Math.max(a, s.sourceStartMs), hi = Math.min(b, s.sourceEndMs);
    if (hi > lo) {
      const o0 = acc + (lo - s.sourceStartMs) / s.timeScale;
      const o1 = acc + (hi - s.sourceStartMs) / s.timeScale;
      const last = out[out.length - 1];
      if (last && prev === s.timeScale && Math.abs(last[1] - o0) < 0.5 && Math.abs(last[3] - lo) < 0.5) {
        last[1] = o1;
        last[3] = hi;
      } else {
        out.push([o0, o1, lo, hi]);
      }
      prev = s.timeScale;
    }
    acc += sliceLen(s);
  }
  return out;
}
