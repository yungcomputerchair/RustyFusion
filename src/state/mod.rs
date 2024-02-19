use std::{sync::mpsc::Receiver, time::SystemTime};

use crate::error::FFResult;

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
}

pub enum ServerState {
    Login(Box<LoginServerState>),
    Shard(Box<ShardServerState>),
}
impl ServerState {
    pub fn new_login() -> Self {
        Self::Login(Box::default())
    }

    pub fn new_shard() -> Self {
        Self::Shard(Box::default())
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
