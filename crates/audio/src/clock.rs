use std::sync::Arc;
use std::time::Instant;

/// The one timeline every stream (video frames, input events, audio) is stamped against.
#[derive(Clone, Debug)]
pub struct Clock {
    epoch: Arc<Instant>,
}

impl Clock {
    pub fn start() -> Self {
        Self { epoch: Arc::new(Instant::now()) }
    }

    /// Nanoseconds since the clock started.
    pub fn now_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

    /// The instant that is time zero.
    pub fn epoch(&self) -> Instant {
        *self.epoch
    }

    /// Nanoseconds for an arbitrary instant (may be before the epoch, then 0).
    pub fn ns_of(&self, t: Instant) -> u64 {
        t.saturating_duration_since(*self.epoch).as_nanos() as u64
    }
}
