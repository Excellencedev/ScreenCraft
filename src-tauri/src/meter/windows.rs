//! System-audio meter on Windows. Not implemented yet (WASAPI loopback later); the
//! toolbar simply shows no level.

use tauri::AppHandle;

#[derive(Default)]
pub struct Meter;

impl Meter {
    pub fn start(&self, _app: AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub fn stop(&self) {}
}
