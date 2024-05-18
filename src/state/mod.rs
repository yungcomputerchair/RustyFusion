use std::{
    sync::mpsc::{Receiver, Sender, TryRecvError},
    time::{Duration, SystemTime},
};

use crate::error::{FFError, FFResult, Severity};

mod login;
pub use login::*;

mod shard;
pub use shard::*;

#[derive(Debug)]
pub struct FFReceiver<T> {
    start_time: SystemTime,
    rx: Receiver<T>,
}
impl<T> FFReceiver<T> {
    pub fn new(start_time: SystemTime, rx: Receiver<T>) -> Self {
        Self { start_time, rx }
    }

    pub fn recv(&self, timeout: Option<Duration>) -> FFResult<T> {
        match timeout {
            Some(timeout) => match self.rx.recv_timeout(timeout) {
                Ok(res) => Ok(res),
                Err(e) => Err(FFError::build(
                    Severity::Warning,
                    format!("Failed to receive result: {}", e),
                )),
            },
            None => match self.rx.recv() {
                Ok(res) => Ok(res),
                Err(e) => Err(FFError::build(
                    Severity::Warning,
                    format!("Failed to receive result: {}", e),
                )),
            },
        }
    }

    pub fn try_recv(&self) -> Option<FFResult<T>> {
        match self.rx.try_recv() {
            Ok(res) => Some(Ok(res)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(FFError::build(
                Severity::Warning,
                format!(
                    "Receiver disconnected (started {}s ago)",
                    self.start_time.elapsed().unwrap_or_default().as_secs()
                ),
            ))),
        }
    }
}

#[derive(Debug)]
pub struct FFSender<T> {
    tx: Sender<T>,
}
impl<T> FFSender<T> {
    pub fn new(tx: Sender<T>) -> Self {
        Self { tx }
    }

    pub fn send(&self, val: T) -> FFResult<()> {
        match self.tx.send(val) {
            Ok(_) => Ok(()),
            Err(e) => Err(FFError::build(
                Severity::Warning,
                format!("Failed to send result: {}", e,),
            )),
        }
    }
}

pub enum ServerState {
    Login(Box<LoginServerState>),
    Shard(Box<ShardServerState>),
}
impl ServerState {
    pub fn new_login() -> Self {
        Self::Login(Box::default())
    }

    pub fn new_shard(shard_id: i32) -> Self {
        Self::Shard(Box::new(ShardServerState::new(shard_id)))
    }

    pub fn as_login(&mut self) -> &mut LoginServerState {
        if let Self::Login(state) = self {
            state
        } else {
            panic!("State is not LoginServerState");
        }
    }

    pub fn as_shard(&mut self) -> &mut ShardServerState {
        if let Self::Shard(state) = self {
            state
        } else {
            panic!("State is not ShardServerState");
        }
    }
}
