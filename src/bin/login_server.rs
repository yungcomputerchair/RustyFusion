use rusty_fusion::{net::cnserver::CNServer, Result};

fn main() -> Result<()> {
    println!("Hello from login server!");
    let mut server: CNServer = CNServer::new(None).unwrap();
    loop {
        server.poll()?;
    }
}
