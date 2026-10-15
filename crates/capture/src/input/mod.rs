//! Cursor, click, key and text-caret logging while recording. Output is the `InputLog` of
//! `screencraft-core`, with times in milliseconds from the first video frame.

#[cfg(target_os = "macos")]
mod ax;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

use screencraft_core::input::InputLog;
use screencraft_core::project::Rect;
use std::time::Instant;

#[cfg(target_os = "macos")]
pub fn accessibility_trusted() -> bool {
    ax::trusted()
}

#[cfg(not(target_os = "macos"))]
pub fn accessibility_trusted() -> bool {
    true
}

pub struct InputTracker(platform::InputTracker);

impl InputTracker {
    /// Start logging. `display_id` selects the coordinate space on platforms that need one.
    pub fn start(display_id: Option<u32>) -> Self {
        #[cfg(target_os = "macos")]
        {
            let _ = display_id;
            Self(platform::InputTracker::start())
        }
        #[cfg(windows)]
        {
            let d = crate::monitors::display(display_id).expect("no display");
            Self(platform::InputTracker::start(d))
        }
        #[cfg(target_os = "linux")]
        {
            Self(platform::InputTracker::start(display_id))
        }
    }

    /// Stop and convert to times relative to `zero`.
    pub fn finish(self, zero: Instant, bounds: Rect) -> InputLog {
        self.0.finish(zero, bounds)
    }
}
