use rusty_fusion::{Result, net::cnserver::CNServer};
use login_server::LoginServer;

mod login_server;

fn main() -> Result<()> {
    println!("Hello from login server!");
    let mut server: LoginServer = LoginServer::new(None);
    server.init()?;
    loop {
        server.poll()?;
    }
}