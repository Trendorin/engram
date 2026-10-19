//! Connections to engramd. Calls run on worker threads (each with its own
//! connection) so the UI thread never blocks; events arrive on a listener
//! thread that reconnects, and auto-starts the daemon, whenever the stream ends.

use engram_proto::client::Client;
use engram_proto::{Event, RpcError};
use serde_json::Value;
use std::sync::mpsc;
use std::time::Duration;

/// Client name the daemon trusts as the user's own window.
pub const CLIENT: &str = "gui";

/// A connection that is opened lazily and replaced after the daemon goes away.
pub struct Conn {
    client: Option<Client>,
}

impl Conn {
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let stale = self.client.is_some();
        let r = self.try_call(method, params.clone());
        match r {
            // A connection kept from before a daemon restart fails on first use;
            // retry once on a fresh connection.
            Err(e) if stale && e.code == "daemon_unavailable" => self.try_call(method, params),
            r => r,
        }
    }

    fn try_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        if self.client.is_none() {
            self.client = Some(Client::connect_or_spawn(CLIENT)?);
        }
        let r = self.client.as_mut().expect("connected above").call(method, params);
        if matches!(&r, Err(e) if e.code == "daemon_unavailable" || e.code == "protocol_error") {
            self.client = None;
        }
        r
    }
}

type Job = Box<dyn FnOnce(&mut Conn) + Send>;

/// A thread that runs jobs one after another on its own connection.
#[derive(Clone)]
pub struct Worker {
    tx: mpsc::Sender<Job>,
}

impl Worker {
    pub fn spawn(name: &str) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut conn = Conn { client: None };
                for job in rx {
                    job(&mut conn);
                }
            })
            .expect("spawn worker thread");
        Self { tx }
    }

    pub fn run(&self, job: impl FnOnce(&mut Conn) + Send + 'static) {
        let _ = self.tx.send(Box::new(job));
    }
}

/// Subscribe as the approver and forward every event. `on_state` reports
/// connects (`Ok`) and losses or failed attempts (`Err`).
pub fn listen(on_state: impl Fn(Result<(), RpcError>) + Send + 'static, on_event: impl Fn(Event) + Send + 'static) {
    std::thread::Builder::new()
        .name("events".into())
        .spawn(move || loop {
            match Client::connect_or_spawn(CLIENT).and_then(|c| c.subscribe(true)) {
                Ok(stream) => {
                    on_state(Ok(()));
                    for e in stream {
                        on_event(e);
                    }
                    on_state(Err(RpcError::new("daemon_unavailable", "the engine stopped; restarting it")));
                }
                Err(e) => on_state(Err(e)),
            }
            std::thread::sleep(Duration::from_secs(1));
        })
        .expect("spawn event thread");
}
