use self::{login::LoginServerState, shard::ShardServerState};

pub mod login;
pub mod shard;

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
