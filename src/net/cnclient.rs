use std::{net::{TcpStream, SocketAddr}, time::SystemTime};

use crate::util::get_time;

pub struct CNClient {
    sock: TcpStream,
    addr: SocketAddr,
    e_key: u64,
    fe_key: u64,
    heartbeat: u64
}

impl CNClient {
    pub fn new(conn_data: (TcpStream, SocketAddr)) -> Self {
        Self {
            sock: conn_data.0,
            addr: conn_data.1,
            e_key: todo!(),
            fe_key: todo!(),
            heartbeat: get_time()
        }
    }
}

