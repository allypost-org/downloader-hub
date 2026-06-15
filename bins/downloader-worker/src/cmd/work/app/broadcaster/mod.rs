use std::sync::{Arc, OnceLock};

use app_peer_comms::message::v1::{common::file::FileReference, worker::WorkerMessage};
use tokio::sync::broadcast;
use tracing::trace;

static BROADCASTER: OnceLock<Broadcaster> = OnceLock::new();

type Broadcast = Arc<WorkerMessage>;

pub struct Broadcaster {
    send: broadcast::Sender<Broadcast>,
}

impl Broadcaster {
    pub fn init() -> Result<(), &'static str> {
        let (send, _) = broadcast::channel::<Broadcast>(60);
        BROADCASTER
            .set(Self { send })
            .map_err(|_| "Failed to init broadcaster")
    }

    pub fn get() -> &'static Self {
        BROADCASTER.get().expect("Broadcaster not initialized")
    }

    pub fn send<T>(&self, msg: T)
    where
        T: Into<Broadcast>,
    {
        _ = self.try_send(msg);
    }

    pub fn try_send<T>(&self, msg: T) -> Result<usize, broadcast::error::SendError<Broadcast>>
    where
        T: Into<Broadcast>,
    {
        let msg = msg.into();

        trace!(?msg, "Broadcasting message");

        self.send.send(msg)
    }

    pub fn recv(&self) -> broadcast::Receiver<Broadcast> {
        self.send.subscribe()
    }
}

impl Broadcaster {
    pub fn send_work_request_take(&self, request_id: Arc<str>) {
        self.send(WorkerMessage::WorkRequestTake { request_id });
    }

    pub fn send_work_request_free(&self, request_id: Arc<str>) {
        self.send(WorkerMessage::WorkRequestFree { request_id });
    }

    pub fn send_work_request_update_status_message(&self, request_id: Arc<str>, message: &str) {
        self.send(WorkerMessage::WorkRequestUpdateStatusMessage {
            request_id,
            message: Arc::from(message),
        });
    }

    pub fn send_work_request_add_errors(&self, request_id: Arc<str>, errors: Vec<String>) {
        self.send(WorkerMessage::WorkRequestAddErrors { request_id, errors });
    }

    pub fn send_work_request_move_to_waiting_for_requester(
        &self,
        request_id: Arc<str>,
        files_data: Vec<FileReference>,
    ) {
        self.send(WorkerMessage::WorkRequestMoveToWaitingForRequester {
            request_id,
            files_data,
        });
    }

    pub fn send_work_request_fail(&self, request_id: Arc<str>, reason: &str) {
        self.send(WorkerMessage::WorkRequestFail {
            request_id,
            reason: Arc::from(reason),
        });
    }
}
