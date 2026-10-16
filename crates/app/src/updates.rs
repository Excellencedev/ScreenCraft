//! Notify-only update check. ScreenCraft asks one small JSON file whether a newer version exists
//! and, when it does, tells the user and offers the download. It never installs anything by
//! itself. The request carries no identifier beyond the app version in the User-Agent, and it can
//! be switched off in About & privacy.

use crate::prefs::Prefs;
use serde::Deserialize;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};

/// The same static file served by two names, the second a fallback for networks that block the first.
const MANIFEST_URLS: &[&str] = &["https://screencraft.mdnlabs.dev/latest.json", "https://getscreencraft.pages.dev/latest.json"];
const ALLOWED_PREFIX: &str = "https://screencraft.mdnlabs.dev/";
const ALLOWED_FALLBACK: &str = "https://getscreencraft.pages.dev/";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The update check is offered on macOS and Windows.
pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    mac: String,
    #[serde(default)]
    windows: String,
    #[serde(default)]
    page: String,
}

#[derive(Clone, Default, Debug)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: Vec<String>,
    pub download: String,
    pub page: String,
    pub error: Option<String>,
}

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-', '+']).map_while(|p| p.parse::<u64>().ok()).collect()
}

pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn fetch() -> UpdateInfo {
    let current = VERSION.to_string();
    let mut last = String::new();
    let mut body = None;
    for url in MANIFEST_URLS {
        match ureq::get(url).set("User-Agent", &format!("ScreenCraft/{current}")).timeout(std::time::Duration::from_secs(8)).call() {
            Ok(r) => match r.into_string() {
                Ok(b) => {
                    body = Some(b);
                    break;
                }
                Err(e) => last = e.to_string(),
            },
            Err(e) => last = e.to_string(),
        }
    }
    let Some(body) = body else {
        return UpdateInfo { current, error: Some(format!("could not reach the update server: {last}")), ..Default::default() };
    };
    let m: Manifest = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => return UpdateInfo { current, error: Some(format!("unreadable update information: {e}")), ..Default::default() },
    };
    let pick = if cfg!(windows) { &m.windows } else { &m.mac };
    // only ever hand out links on our own site
    let ok = |u: &str| u.starts_with(ALLOWED_PREFIX) || u.starts_with(ALLOWED_FALLBACK);
    let page = if ok(&m.page) { m.page.clone() } else { format!("{ALLOWED_PREFIX}download") };
    let download = if ok(pick) { pick.clone() } else { page.clone() };
    UpdateInfo {
        available: newer(&m.version, &current),
        current,
        latest: m.version,
        notes: m.notes.into_iter().take(6).collect(),
        download,
        page,
        error: None,
    }
}

/// What the startup check found, so the editor can show its badge without asking again.
static FOUND: OnceLock<Mutex<Option<UpdateInfo>>> = OnceLock::new();

fn found() -> &'static Mutex<Option<UpdateInfo>> {
    FOUND.get_or_init(|| Mutex::new(None))
}

pub fn enabled() -> bool {
    Prefs::load().update_check.unwrap_or(true)
}

pub fn set_enabled(on: bool) {
    let mut p = Prefs::load();
    p.update_check = Some(on);
    p.save();
}

pub fn dismiss(version: &str) {
    let mut p = Prefs::load();
    p.dismissed_update = Some(version.to_string());
    p.save();
    if let Some(f) = found().lock().unwrap().as_mut() {
        f.available = false;
    }
}

/// Run once at startup on a worker thread.
pub fn startup_check(ctx: eframe::egui::Context) {
    if !SUPPORTED || !enabled() {
        return;
    }
    std::thread::spawn(move || {
        let info = fetch();
        if info.available && Prefs::load().dismissed_update.as_deref() != Some(info.latest.as_str()) {
            *found().lock().unwrap() = Some(info);
            ctx.request_repaint();
        }
    });
}

/// A newer version found at startup that the user has not dismissed.
pub fn update_found() -> Option<UpdateInfo> {
    found().lock().unwrap().clone().filter(|i| i.available)
}

pub fn remember(info: &UpdateInfo) {
    *found().lock().unwrap() = Some(info.clone());
}

/// Download the installer into Downloads and open it. `progress` receives 0..100.
pub fn download(url: &str, version: &str, progress: &dyn Fn(f32)) -> Result<std::path::PathBuf, String> {
    if !(url.starts_with(ALLOWED_PREFIX) || url.starts_with(ALLOWED_FALLBACK)) {
        return Err("that link is not on the ScreenCraft site".into());
    }
    let name = url.rsplit('/').next().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| format!("ScreenCraft-{version}"));
    let dir = dirs::download_dir().ok_or("no Downloads folder")?;
    let dest = dir.join(&name);
    let resp = ureq::get(url).set("User-Agent", &format!("ScreenCraft/{VERSION}")).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| e.to_string())?;
    let total: f32 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let mut reader = resp.into_reader();
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0f32;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as f32;
        if total > 0.0 {
            progress(got / total * 100.0);
        }
    }
    drop(out);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    let _ = crate::editor::open_url(&dest.to_string_lossy());
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.4.1", "0.4.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("0.4.0", "0.4.0"));
        assert!(!newer("0.3.9", "0.4.0"));
        assert!(newer("1.0", "0.9.9"));
    }
}
