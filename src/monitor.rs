use std::{
    collections::{HashMap, HashSet},
    io::Write as _,
    net::TcpListener,
    sync::{
        mpsc::{self, Receiver, Sender},
        OnceLock,
    },
    time::Duration,
};

use ffmonitor::{Event, MonitorUpdate};

use crate::error::{log, Severity};

pub type MonitorEvent = Event;

static FEED: OnceLock<Sender<MonitorEvent>> = OnceLock::new();

pub fn monitor_init(addr: String, interval: Duration) -> &'static Sender<MonitorEvent> {
    assert!(FEED.get().is_none());
    let (tx, rx) = mpsc::channel();
    FEED.set(tx).unwrap();
    std::thread::spawn(move || monitor_thread(rx, addr, interval));
    monitor_get()
}

pub fn monitor_get() -> &'static Sender<MonitorEvent> {
    FEED.get().unwrap()
}

fn monitor_thread(rx: Receiver<MonitorEvent>, addr: String, interval: Duration) {
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(e) => {
            log(
                Severity::Warning,
                &format!("Failed to start monitor feed: {}", e),
            );
            return;
        }
    };

    if let Err(e) = listener.set_nonblocking(true) {
        log(
            Severity::Warning,
            &format!(
                "Failed to set monitor feed to non-blocking; aborting: {}",
                e
            ),
        );
        return;
    }

    log(Severity::Info, &format!("Monitor feed started on {}", addr));

    let mut clients = HashMap::new();
    let mut to_disconnect = HashSet::new();
    loop {
        std::thread::sleep(interval);

        while let Ok((client, client_addr)) = listener.accept() {
            log(
                Severity::Info,
                &format!("Monitor connected: {}", client_addr),
            );
            clients.insert(client_addr, client);
        }

        let mut update = MonitorUpdate::default();
        while let Ok(event) = rx.try_recv() {
            update.add_event(event);
        }

        for (client_addr, client) in clients.iter_mut() {
            if client.write_all(&update.to_string().into_bytes()).is_err() {
                to_disconnect.insert(*client_addr);
            }
        }

        for client_addr in to_disconnect.drain() {
            clients.remove(&client_addr);
            log(
                Severity::Info,
                &format!("Monitor disconnected: {}", client_addr),
            );
        }
    }
}
