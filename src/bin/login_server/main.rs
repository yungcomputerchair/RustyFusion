extern crate bcrypt;

use login_server::LoginServer;
use rusty_fusion::{net::cnserver::CNServer, Result};

mod login_server;

fn main() -> Result<()> {
    println!("Hello from login server!");
    let mut server: LoginServer = LoginServer::new(None).unwrap();
    loop {
        server.poll()?;
    }
}
