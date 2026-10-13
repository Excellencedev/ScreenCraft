// "Coming soon": features we plan to add (see docs/feature-research.md). Nothing here
// changes the project; it is a roadmap the user can browse from inside the editor.
import { h3, el } from "../widgets.js";

const GROUPS = [
  {
    title: "Editing",
    items: [
      ["Keyframes", "Animate the camera bubble and annotations over time."],
      ["Colour adjustments", "Brightness, contrast, saturation and looks for the screen layer."],
    ],
  },
  {
    title: "Look & export",
    items: [
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
