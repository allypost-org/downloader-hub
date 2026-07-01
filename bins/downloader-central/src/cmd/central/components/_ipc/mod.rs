use std::sync::{Arc, OnceLock};

use tokio::sync::{RwLock, broadcast};
use tracing::trace;

pub type Receiver = broadcast::Receiver<Arc<IpcMessage>>;
pub type Sender = broadcast::Sender<Arc<IpcMessage>>;

static COMPONENT_IPC_SENDER: OnceLock<Sender> = OnceLock::new();
static COMPONENT_IPC_RECEIVER: OnceLock<Arc<RwLock<Receiver>>> = OnceLock::new();

#[derive(Debug)]
pub enum IpcMessage {
    DatabaseReady,
    PeersReady,
    WorkerApiReady,
    WorkerRequests(Arc<[app_database::api::requests::RequestInfoResponse]>),
}

impl IpcMessage {
    pub fn send(self) -> Result<usize, broadcast::error::SendError<Arc<Self>>> {
        trace!(msg = ?self, "Sending IPC message");

        let sender = COMPONENT_IPC_SENDER.get().expect("Sender not initialized");
        sender.send(self.into())
    }

    pub fn recv_from_now() -> Receiver {
        let receiver = COMPONENT_IPC_SENDER.get().expect("Sender not initialized");

        receiver.subscribe()
    }
}

pub(super) fn init() {
    let (send, recv) = broadcast::channel(1024);

    _ = COMPONENT_IPC_SENDER.set(send);
    _ = COMPONENT_IPC_RECEIVER.set(Arc::new(RwLock::new(recv)));
}
