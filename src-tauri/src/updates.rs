//! Notify-only update check. ScreenCraft asks one small JSON file whether a newer version
//! exists and, when it does, tells the user and offers the download page. It never downloads
//! or installs anything by itself. The request carries no identifier beyond the app version in
//! the User-Agent; it can be switched off in About & privacy.

use crate::state::{save_settings, AppState, CmdResult};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tauri::{AppHandle, Emitter, State};

/// The same static file, served by the same Cloudflare Pages project on both names; the second
/// is a fallback for networks that block the custom domain.
const MANIFEST_URLS: &[&str] = &["https://screencraft.mdnlabs.dev/latest.json", "https://getscreencraft.pages.dev/latest.json"];
const ALLOWED_PREFIX: &str = "https://screencraft.mdnlabs.dev/";
const ALLOWED_FALLBACK: &str = "https://getscreencraft.pages.dev/";

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

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: Vec<String>,
    pub download: String,
    pub page: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrefs {
    pub supported: bool,
    pub enabled: bool,
    pub version: String,
    pub dismissed: Option<String>,
}

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-', '+']).map_while(|p| p.parse::<u64>().ok()).collect()
}

fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn fetch(current: &str) -> Result<UpdateInfo, String> {
    let mut last = String::new();
    let mut got = None;
    for url in MANIFEST_URLS {
        match ureq::get(url)
            .set("User-Agent", &format!("ScreenCraft/{current}"))
            .timeout(std::time::Duration::from_secs(8))
            .call()
        {
            Ok(r) => { got = Some(r); break; }
            Err(e) => last = e.to_string(),
        }
    }
    let resp = got.ok_or_else(|| format!("could not reach the update server: {last}"))?;
    let body = resp.into_string().map_err(|e| format!("unreadable update information: {e}"))?;
    let m: Manifest = serde_json::from_str(&body).map_err(|e| format!("unreadable update information: {e}"))?;
    let pick = if cfg!(windows) { &m.windows } else { &m.mac };
    // only ever hand out links on our own site
    let ok = |u: &str| u.starts_with(ALLOWED_PREFIX) || u.starts_with(ALLOWED_FALLBACK);
    let page = if ok(&m.page) { m.page.clone() } else { format!("{ALLOWED_PREFIX}download") };
    let download = if ok(pick) { pick.clone() } else { page.clone() };
    Ok(UpdateInfo {
        available: newer(&m.version, current),
        current: current.to_string(),
        latest: m.version,
        notes: m.notes.into_iter().take(6).collect(),
        download,
        page,
        error: None,
    })
}

pub fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// The update check is macOS-only for now.
pub const SUPPORTED: bool = cfg!(target_os = "macos");

/// What the startup check found, so windows can show a badge without asking the network again.
static FOUND: std::sync::Mutex<Option<UpdateInfo>> = std::sync::Mutex::new(None);

pub fn enabled(state: &AppState) -> bool {
    state.settings.lock().unwrap().update_check.unwrap_or(true)
}

/// Blocking check used at startup. None when disabled or nothing newer.
pub fn startup_check(app: &AppHandle, state: &AppState) -> Option<UpdateInfo> {
    if !SUPPORTED || !enabled(state) {
        return None;
    }
    let info = fetch(&current_version(app)).ok()?;
    if !info.available {
        return None;
    }
    let dismissed = state.settings.lock().unwrap().dismissed_update.clone();
    if dismissed.as_deref() == Some(info.latest.as_str()) {
        return None;
    }
    *FOUND.lock().unwrap() = Some(UpdateInfo { notes: info.notes.clone(), current: info.current.clone(), latest: info.latest.clone(), download: info.download.clone(), page: info.page.clone(), available: true, error: None });
    Some(info)
}

#[tauri::command]
pub fn update_prefs(app: AppHandle, state: State<AppState>) -> UpdatePrefs {
    UpdatePrefs {
        supported: SUPPORTED,
        enabled: enabled(&state),
        version: current_version(&app),
        dismissed: state.settings.lock().unwrap().dismissed_update.clone(),
    }
}

/// A newer version found at startup that the user has not dismissed (None otherwise).
#[tauri::command]
pub fn update_found(state: State<AppState>) -> Option<UpdateInfo> {
    let found = FOUND.lock().unwrap().clone_info()?;
    let dismissed = state.settings.lock().unwrap().dismissed_update.clone();
    if dismissed.as_deref() == Some(found.latest.as_str()) { None } else { Some(found) }
}

trait CloneInfo { fn clone_info(&self) -> Option<UpdateInfo>; }
impl CloneInfo for Option<UpdateInfo> {
    fn clone_info(&self) -> Option<UpdateInfo> {
        self.as_ref().map(|i| UpdateInfo { available: i.available, current: i.current.clone(), latest: i.latest.clone(), notes: i.notes.clone(), download: i.download.clone(), page: i.page.clone(), error: None })
    }
}

#[tauri::command]
pub fn set_update_check(state: State<AppState>, enabled: bool) {
    let mut s = state.settings.lock().unwrap();
    s.update_check = Some(enabled);
    save_settings(&s);
}

#[tauri::command]
pub fn dismiss_update(state: State<AppState>, version: String) {
    let mut s = state.settings.lock().unwrap();
    s.dismissed_update = Some(version);
    save_settings(&s);
}

/// "Check now": always runs, whatever the startup setting is, because the user asked.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> CmdResult<UpdateInfo> {
    let current = current_version(&app);
    if !SUPPORTED {
        return Ok(UpdateInfo { current, ..Default::default() });
    }
    tauri::async_runtime::spawn_blocking(move || match fetch(&current) {
        Ok(i) => Ok(i),
        Err(e) => Ok(UpdateInfo { current, error: Some(e), ..Default::default() }),
    })
    .await
    .map_err(|e| e.to_string())?
}

/// User-initiated download of the new installer, done by the app itself (not a browser), so
/// macOS does not tag it with the quarantine flag and the new copy opens without the
/// "Apple could not verify..." prompt. Nothing is installed: the disk image is saved to
/// Downloads and opened, and the user drags the app across themselves.
#[tauri::command]
pub async fn download_update(app: AppHandle, url: String, version: String) -> CmdResult<String> {
    let ok = |u: &str| (u.starts_with(ALLOWED_PREFIX) || u.starts_with(ALLOWED_FALLBACK)) && u.ends_with(".dmg");
    if !ok(&url) {
        return Err("unexpected download address".into());
    }
    let safe: String = version.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    tauri::async_runtime::spawn_blocking(move || {
        let dest_dir = crate::state::home().join("Downloads");
        std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
        let dest = dest_dir.join(format!("ScreenCraft {safe}.dmg"));
        let part = dest.with_extension("dmg.part");
        // the custom domain first, the pages.dev name if the network blocks it
        let candidates = [url.clone(), url.replace(ALLOWED_PREFIX, ALLOWED_FALLBACK)];
        let mut last = String::new();
        let mut resp = None;
        for u in candidates.iter() {
            match ureq::get(u).set("User-Agent", "ScreenCraft-updater").timeout(std::time::Duration::from_secs(30)).call() {
                Ok(r) => { resp = Some(r); break; }
                Err(e) => last = e.to_string(),
            }
        }
        let resp = resp.ok_or_else(|| format!("download failed: {last}"))?;
        let total = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let mut reader = resp.into_reader();
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        let (mut done, mut last_pct) = (0u64, -1i64);
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
            if n == 0 { break; }
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if total > 0 {
                let pct = (done as f64 / total as f64 * 100.0).min(100.0);
                if pct as i64 != last_pct {
                    last_pct = pct as i64;
                    let _ = app.emit("update-download", pct);
                }
            }
        }
        out.flush().map_err(|e| e.to_string())?;
        drop(out);
        if done < 1_000_000 {
            let _ = std::fs::remove_file(&part);
            return Err("the download was incomplete".into());
        }
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&dest).spawn();
        Ok(dest.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_versions() {
        assert!(newer("0.3.1", "0.3.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.3.0", "0.3.0"));
        assert!(!newer("0.2.9", "0.3.0"));
    }
}
