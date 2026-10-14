import { emit, invoke } from "./tauri.js";
const q = new URLSearchParams(location.search);
const ox = parseFloat(q.get("x") || "0"), oy = parseFloat(q.get("y") || "0");
const did = q.get("d") ? Number(q.get("d")) : null;
const closeAll = () => invoke("close_area_picker");
const sel = document.getElementById("sel"), lab = document.getElementById("lab");
let start = null, rect = null;

function show(r) {
  Object.assign(sel.style, { display: "block", left: r.x + "px", top: r.y + "px", width: r.w + "px", height: r.h + "px" });
  lab.style.display = "block";
  lab.textContent = `${Math.round(r.w)} × ${Math.round(r.h)}`;
  lab.style.left = r.x + "px";
  lab.style.top = Math.max(0, r.y - 28) + "px";
}
addEventListener("mousedown", (e) => { start = { x: e.clientX, y: e.clientY }; rect = null; document.getElementById("hint").style.display = "none"; });
addEventListener("mousemove", (e) => {
  if (!start) return;
  const x = Math.min(start.x, e.clientX), y = Math.min(start.y, e.clientY);
  rect = { x, y, w: Math.abs(e.clientX - start.x), h: Math.abs(e.clientY - start.y) };
  show(rect);
});
addEventListener("mouseup", async () => {
  start = null;
  if (rect && rect.w > 40 && rect.h > 40) {
    await emit("area-selected", { x: ox + rect.x, y: oy + rect.y, width: rect.w, height: rect.h, displayId: did });
  } else {
    await emit("area-cancelled", {});
  }
  closeAll();
});
addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    await emit("area-cancelled", {});
    closeAll();
  }
});
