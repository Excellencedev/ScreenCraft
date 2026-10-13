//! Screen capture on Windows. Not implemented yet: starting a recording reports
//! that clearly instead of failing somewhere deeper.

use super::{StartInfo, StartOpts, StopInfo};

#[derive(Clone, Default)]
pub struct Capture;

impl Capture {
    pub fn spawn() -> Self {
        Self
    }

    pub fn start(&self, _opts: StartOpts) -> Result<StartInfo, String> {
        Err("Screen recording is not available on Windows yet".into())
    }

    pub fn stop(&self) -> Result<StopInfo, String> {
        Err("not recording".into())
    }
}
