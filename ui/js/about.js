import { invoke } from "./tauri.js";

const $ = (id) => document.getElementById(id);
let info = null;

const open = (url) => invoke("plugin:opener|open_url", { url }).catch(() => {});
document.querySelectorAll("a[data-url]").forEach((a) => { a.onclick = () => open(a.dataset.url); });

function render(i) {
  info = i;
  const card = $("updateCard");
  card.hidden = !(i && i.available);
  if (i && i.available) {
    $("uTitle").textContent = `ScreenCraft ${i.latest} is available (you have ${i.current})`;
    $("uNotes").replaceChildren(...(i.notes || []).map((n) => Object.assign(document.createElement("li"), { textContent: n })));
    $("uNotes").hidden = !(i.notes || []).length;
  }
  const st = $("uStatus");
  st.classList.toggle("err", !!(i && i.error));
  if (!i) st.textContent = "Not checked yet.";
  else if (i.error) st.textContent = "Could not check for updates. You are offline or the server is unreachable.";
  else if (i.available) st.textContent = `Version ${i.latest} is available.`;
  else st.textContent = `You have the latest version (${i.current}).`;
}

async function check() {
  $("uStatus").textContent = "Checking…";
  $("bCheck").disabled = true;
  try { render(await invoke("check_for_update")); } finally { $("bCheck").disabled = false; }
}

$("bCheck").onclick = check;
$("bDownload").onclick = () => info && open(info.download || info.page);
$("bLater").onclick = async () => {
  if (info) await invoke("dismiss_update", { version: info.latest });
  $("updateCard").hidden = true;
};
$("tCheck").onclick = async () => {
  const on = !$("tCheck").classList.contains("on");
  $("tCheck").classList.toggle("on", on);
  $("tCheck").setAttribute("aria-checked", String(on));
  await invoke("set_update_check", { enabled: on });
};

(async () => {
  const p = await invoke("update_prefs");
  $("ver").textContent = `Version ${p.version}`;
  $("tCheck").classList.toggle("on", p.enabled);
  $("tCheck").setAttribute("aria-checked", String(p.enabled));
  // opening this window is itself a request to know, so look straight away
  check();
})();
