use std::{
    sync::mpsc::{Receiver, Sender, TryRecvError},
    time::SystemTime,
};

use crate::error::{FFError, FFResult, Severity};

mod login;
pub use login::*;

mod shard;
pub use shard::*;

#[derive(Debug)]
pub struct FFReceiver<T> {
    start_time: SystemTime,
    rx: Receiver<FFResult<T>>,
}
impl<T> FFReceiver<T> {
    pub fn new(start_time: SystemTime, rx: Receiver<FFResult<T>>) -> Self {
        Self { start_time, rx }
    }

    pub fn try_recv(&self) -> Option<FFResult<T>> {
        match self.rx.try_recv() {
            Ok(res) => Some(res),
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
pub struct FFTransmitter<T> {
    tx: Sender<FFResult<T>>,
}
impl<T> FFTransmitter<T> {
    pub fn new(tx: Sender<FFResult<T>>) -> Self {
        Self { tx }
    }

    pub fn send(&self, result: FFResult<T>) -> FFResult<()> {
        match self.tx.send(result) {
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
