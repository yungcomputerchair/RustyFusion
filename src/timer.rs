use std::time::Duration;

use tokio::time::{interval_at, Interval, MissedTickBehavior};

pub fn make_timer(dur: Duration, prime: bool) -> Interval {
    let start = if prime {
        tokio::time::Instant::now()
    } else {
        tokio::time::Instant::now() + dur
    };
    let mut iv = interval_at(start, dur);
    iv.set_missed_tick_behavior(MissedTickBehavior::Delay);
    iv
}
