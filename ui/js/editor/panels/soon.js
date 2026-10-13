// "Coming soon": features we plan to add (see docs/feature-research.md). Nothing here
// changes the project; it is a roadmap the user can browse from inside the editor.
import { h3, el } from "../widgets.js";

const GROUPS = [
  {
    title: "Annotate",
    items: [
      ["Text & callouts", "Titles, arrows and boxes with fade or pop animations, placed on the timeline."],
      ["Blur & spotlight", "Hide passwords and personal info, or dim everything except one area."],
      ["Keystroke overlay", "Show shortcuts like ⌘⇧4 as a pill while you press them."],
    ],
  },
  {
    title: "Pace & transitions",
    items: [
      ["Speed up idle parts", "Find stretches with no clicks, typing or speech and fast-forward them."],
      ["Speed ramps", "Ease into and out of fast-forward instead of cutting speed abruptly."],
      ["Transitions", "Fade, dissolve, wipe and cross-zoom between clips."],
      ["Freeze frame", "Hold one frame for a moment, for example to point something out."],
    ],
  },
  {
    title: "Sound & captions",
    items: [
      ["Fades & ducking", "Fade music in and out and lower it automatically while you speak."],
      ["Subtitles", "Type or import SRT captions with styles and burn them into the video."],
    ],
  },
  {
    title: "Look & export",
    items: [
      ["Camera layouts", "Camera-only, side-by-side, or large and small for different parts of the video."],
      ["Title cards & logo", "Intro and outro slides and a watermark in the corner."],
      ["Green screen", "Key out a plain background behind you on the camera."],
      ["Platform presets", "One-click export settings for YouTube, X and vertical shorts."],
      ["Device frames", "Put the recording inside a browser window or laptop frame."],
    ],
  },
];

export function panelSoon(p) {
  p.append(h3("Coming soon"));
  p.append(el('<div class="muted">Planned features. None of these change your project yet.</div>'));
  for (const g of GROUPS) {
    p.append(el(`<div class="soon-group">${g.title}</div>`));
    for (const [name, desc] of g.items) {
      const c = el('<div class="soon-card"><div class="soon-top"><b></b><span class="soon-badge">Soon</span></div><div class="soon-desc"></div></div>');
      c.querySelector("b").textContent = name;
      c.querySelector(".soon-desc").textContent = desc;
      p.append(c);
    }
  }
}
