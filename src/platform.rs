use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn now_epoch_seconds(&self) -> u64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}

pub trait UuidGenerator: Send + Sync {
    fn new_uuid(&self) -> String;
}
