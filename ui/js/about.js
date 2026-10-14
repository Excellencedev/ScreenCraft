import { invoke, listen } from "./tauri.js";

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
$("bDownload").onclick = async () => {
  if (!info) return;
  const btn = $("bDownload");
  btn.disabled = true;
  const off = await listen("update-download", (e) => { btn.textContent = `Downloading ${Math.round(e.payload)}%`; });
  btn.textContent = "Downloading…";
  try {
    const path = await invoke("download_update", { url: info.download, version: info.latest });
    btn.textContent = "Downloaded";
    $("uHelp").textContent = `Saved to ${path} and opened. Quit ScreenCraft, drag it onto Applications and replace the old copy, then start it again.`;
  } catch (err) {
    btn.disabled = false;
    btn.textContent = "Download";
    $("uHelp").textContent = `The download did not finish (${err}). You can get it from the website instead.`;
    open(info.page);
  } finally { off?.(); }
};
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
  if (!p.supported) {
    // update notifications are macOS-only for now: nothing to check, nothing sent
    $("updates").hidden = true;
    $("privUpdate").hidden = true;
    $("privNet").textContent = "One thing uses the network, and only when you ask:";
    return;
  }
  $("tCheck").classList.toggle("on", p.enabled);
  $("tCheck").setAttribute("aria-checked", String(p.enabled));
  // opening this window is itself a request to know, so look straight away
  check();
})();
