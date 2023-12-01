use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use crate::error::{FFError, FFResult};

struct Timer<F>
where
    F: FnMut(SystemTime) -> FFResult<()>,
{
    callback: F,
    interval: Duration,
    last_fire: SystemTime,
}
impl<F> Timer<F>
where
    F: FnMut(SystemTime) -> FFResult<()>,
{
    fn new(callback: F, interval: Duration, prime: bool) -> Self {
        let last_fire = if prime {
            SystemTime::UNIX_EPOCH
        } else {
            SystemTime::now()
        };
        Self {
            callback,
            interval,
            last_fire,
        }
    }

    fn check(&mut self, time_now: SystemTime) -> Option<FFResult<()>> {
        if time_now.duration_since(self.last_fire).unwrap_or_default() >= self.interval {
            self.last_fire = time_now;
            Some((self.callback)(time_now))
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.last_fire = SystemTime::now();
    }
}

pub struct TimerMap<F>
where
    F: FnMut(SystemTime) -> FFResult<()>,
{
    timers: HashMap<usize, Timer<F>>,
    next_timer_id: usize,
}
impl<F> Default for TimerMap<F>
where
    F: FnMut(SystemTime) -> FFResult<()>,
{
    fn default() -> Self {
        Self {
            timers: HashMap::new(),
            next_timer_id: 1,
        }
    }
}

impl<F> TimerMap<F>
where
    F: FnMut(SystemTime) -> FFResult<()>,
{
    pub fn register_timer(&mut self, callback: F, interval: Duration, prime: bool) -> usize {
        let key = self.next_timer_id;
        self.next_timer_id += 1;
        self.timers
            .insert(key, Timer::new(callback, interval, prime));
        key
    }

    pub fn check_all(&mut self) -> FFResult<()> {
        let time_now = SystemTime::now();
        self.timers.iter_mut().try_for_each(|(key, timer)| {
            if let Some(res) = timer.check(time_now) {
                res.map_err(|e| {
                    FFError::build(e.get_severity(), format!("Timer #{}: {}", key, e.get_msg()))
                })
            } else {
                Ok(())
            }
        })
    }

    pub fn reset_all(&mut self) {
        self.timers.iter_mut().for_each(|(_, timer)| {
            timer.reset();
        });
    }

    pub fn reset(&mut self, timer_id: usize) {
        let timer = self
            .timers
            .get_mut(&timer_id)
            .unwrap_or_else(|| panic!("Timer with id {} not found", timer_id));
        timer.reset();
    }
}
