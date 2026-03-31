mod login;
pub use login::*;

mod shard;
pub use shard::*;

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

    pub fn as_login(&self) -> &LoginServerState {
        if let Self::Login(state) = self {
            state
        } else {
            panic!("State is not LoginServerState");
        }
    }

    pub fn as_shard(&self) -> &ShardServerState {
        if let Self::Shard(state) = self {
            state
        } else {
            panic!("State is not ShardServerState");
        }
    }

    pub fn as_login_mut(&mut self) -> &mut LoginServerState {
        if let Self::Login(state) = self {
            state
        } else {
            panic!("State is not LoginServerState");
        }
    }

    pub fn as_shard_mut(&mut self) -> &mut ShardServerState {
        if let Self::Shard(state) = self {
            state
        } else {
            panic!("State is not ShardServerState");
        }
    }
}
