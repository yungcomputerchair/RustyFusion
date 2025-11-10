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

use crate::{
    error::{log, FFError, FFResult, Severity},
    net::packet::sP_FE2LS_UPDATE_MONITOR,
    util,
};

pub type MonitorEvent = Event;

static FEED: OnceLock<Sender<MonitorEvent>> = OnceLock::new();
static FLUSH_SIGNAL: OnceLock<Sender<()>> = OnceLock::new();

pub fn monitor_init(addr: String) {
    assert!(FEED.get().is_none());
    let (ftx, frx) = mpsc::channel();
    let (stx, srx) = mpsc::channel();
    FEED.set(ftx).unwrap();
    FLUSH_SIGNAL.set(stx).unwrap();
    std::thread::spawn(move || monitor_thread(frx, srx, addr));
}

pub fn monitor_queue(event: MonitorEvent) {
    // for ease of use, it's okay to call this function if the monitor is not initialized
    if let Some(feed) = FEED.get() {
        if feed.send(event).is_err() {
            log(Severity::Warning, "Failed to queue monitor event");
        }
    } else {
        log(
            Severity::Warning,
            "Monitor event queued but monitor not initialized",
        );
    }
}

pub fn monitor_flush() -> FFResult<()> {
    let Some(stx) = FLUSH_SIGNAL.get() else {
        return Err(FFError::build(
            Severity::Warning,
            "Monitor flushed while not initialized".to_string(),
        ));
    };
    if stx.send(()).is_err() {
        return Err(FFError::build(
            Severity::Warning,
            "Failed to signal monitor flush".to_string(),
        ));
    }
    Ok(())
}

pub fn monitor_update_to_packet(update: MonitorUpdate) -> FFResult<sP_FE2LS_UPDATE_MONITOR> {
    let s = update.to_string();
    let pkt = sP_FE2LS_UPDATE_MONITOR {
        szUpdate: util::encode_utf16(&s)?,
    };
    Ok(pkt)
}

pub fn monitor_event_to_packet(event: MonitorEvent) -> FFResult<sP_FE2LS_UPDATE_MONITOR> {
    let mut update = MonitorUpdate::default();
    update.add_event(event);
    monitor_update_to_packet(update)
}

pub fn monitor_update_from_packet(pkt: &sP_FE2LS_UPDATE_MONITOR) -> FFResult<MonitorUpdate> {
    let s = util::parse_utf16(&pkt.szUpdate)?;
    let mut lines = s.lines().map(|ln| ln.to_string()).collect();
    let update = MonitorUpdate::from_lines(&mut lines);
    Ok(update)
}

fn monitor_thread(frx: Receiver<MonitorEvent>, srx: Receiver<()>, addr: String) {
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
        std::thread::sleep(Duration::from_millis(100));

        while let Ok((client, client_addr)) = listener.accept() {
            log(
                Severity::Info,
                &format!("Monitor connected: {}", client_addr),
            );
            clients.insert(client_addr, client);
        }

        if srx.try_recv().is_err() {
            continue;
        }

        let mut update = MonitorUpdate::default();
        while let Ok(event) = frx.try_recv() {
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
