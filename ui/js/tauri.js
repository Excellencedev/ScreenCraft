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
