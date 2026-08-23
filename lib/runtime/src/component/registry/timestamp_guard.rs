use crate::{machine::CURRENT_DISPATCH_TIMESTAMP, scheduler::Period};

pub(super) struct TimestampGuard {
    previous: Option<Period>,
}

impl TimestampGuard {
    #[inline]
    pub fn enter(timestamp: Period) -> Self {
        let previous = CURRENT_DISPATCH_TIMESTAMP.replace(Some(timestamp));

        Self { previous }
    }
}

impl Drop for TimestampGuard {
    #[inline]
    fn drop(&mut self) {
        CURRENT_DISPATCH_TIMESTAMP.set(self.previous);
    }
}
