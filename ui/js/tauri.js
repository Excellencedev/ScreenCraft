// Thin wrappers over the globals injected by Tauri (withGlobalTauri).
const T = window.__TAURI__;
export const invoke = (cmd, args, opts) => T.core.invoke(cmd, args, opts);
export const listen = (name, cb) => T.event.listen(name, cb);
export const emit = (name, payload) => T.event.emit(name, payload);
export const convertFileSrc = (p) => T.core.convertFileSrc(p);
export const currentWindow = () => T.window.getCurrentWindow();
export const LogicalSize = T.window.LogicalSize;
export const LogicalPosition = T.window.LogicalPosition;
export const dialog = T.dialog;
export const IS_WINDOWS = /Windows/.test(navigator.userAgent);
// Custom schemes are served as http://<scheme>.localhost by WebView2 on Windows.
export const SC_BASE = IS_WINDOWS ? "http://sc.localhost" : "sc://localhost";
export const FILE_MANAGER = IS_WINDOWS ? "Explorer" : "Finder";

// The pages are written with macOS wording; on Windows swap in the local names.
if (IS_WINDOWS) {
  const fix = (s) => s.replace(/Finder/g, "Explorer").replace(/⇧⌘/g, "Ctrl+Shift+").replace(/⌘⇧/g, "Ctrl+Shift+").replace(/⌘/g, "Ctrl+");
  for (const el of document.querySelectorAll("[title]")) el.title = fix(el.title);
  const walk = document.createTreeWalker(document.body ?? document.documentElement, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) if (/Finder|⌘/.test(n.nodeValue)) n.nodeValue = fix(n.nodeValue);
}
/** Last component of a file path, with either separator. */
export const baseName = (p) => p.split(/[\\/]/).pop();
