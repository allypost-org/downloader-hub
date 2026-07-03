use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use app_database::{Database, api::authed::AuthedInfoResponse, entity::authed::AuthedForRole};
use app_peer_comms::{
    IrohAcceptError as AcceptError, IrohConnection as Connection,
    IrohProtocolHandler as ProtocolHandler,
    irpc::WithChannels,
    irpc_iroh,
    message::v1::central::{
        add_errors_result::{AddErrorsResult, AddErrorsResultStatus},
        create_result::{CreateResult, CreateResultData},
        fail_result::{FailResult, FailResultStatus},
        finish_result::FinishResult,
        get_work_item_result::GetWorkItemResult,
        move_to_waiting_for_requester_result::{
            MoveToWaitingForRequesterResult, MoveToWaitingForRequesterResultStatus,
        },
        take_result::FreeResult,
        update_status_message_result::{
            UpdateStatusMessageResult, UpdateStatusMessageResultStatus,
        },
        work_request::request::WorkRequest,
        work_request_snapshot::WorkRequestSnapshot,
    },
    rpc::{
        AuthResult, CentralProtocol, CentralRequest,
        request::{Capabilities, CapabilitiesSummary},
    },
};
use arc_swap::ArcSwapOption;
use futures::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

use crate::cmd::central::components::{metrics, rpc::session::SessionRegistry};

mod distributor;
mod revocation;
mod session;

pub use distributor::WorkDistributor;
pub use revocation::run as run_revocation_watcher;

static SESSIONS: OnceLock<SessionRegistry> = OnceLock::new();

pub fn sessions() -> &'static SessionRegistry {
    SESSIONS
        .get()
        .expect("irpc session registry not initialized")
}

pub fn init_sessions() {
    _ = SESSIONS.set(SessionRegistry::default());
}

static DISTRIBUTOR: LazyLock<ArcSwapOption<WorkDistributor>> = LazyLock::new(ArcSwapOption::empty);

static INITIAL_DISTRIBUTOR_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub fn init_distributor() {
    let (handle, join) = WorkDistributor::spawn();
    DISTRIBUTOR.store(Some(Arc::new(handle)));
    *INITIAL_DISTRIBUTOR_HANDLE
        .lock()
        .expect("distributor handle lock poisoned") = Some(join);
}

pub fn take_initial_distributor_handle() -> Option<JoinHandle<()>> {
    INITIAL_DISTRIBUTOR_HANDLE
        .lock()
        .expect("distributor handle lock poisoned")
        .take()
}

pub fn respawn_distributor() -> JoinHandle<()> {
    let (handle, join) = WorkDistributor::spawn();
    DISTRIBUTOR.store(Some(Arc::new(handle)));
    join
}

pub fn distributor() -> Arc<WorkDistributor> {
    DISTRIBUTOR
        .load_full()
        .expect("work distributor not initialized")
}

static CENTRAL_ID: OnceLock<String> = OnceLock::new();

pub fn init_central_id(id: String) {
    _ = CENTRAL_ID.set(id);
}

pub fn central_id() -> String {
    CENTRAL_ID
        .get()
        .expect("central_id not initialized")
        .clone()
}

#[derive(Clone, Debug)]
pub struct CentralRpcServer {
    registry: SessionRegistry,
}

impl CentralRpcServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: sessions().clone(),
        }
    }
}

impl Default for CentralRpcServer {
    fn default() -> Self {
        Self::new()
    }
}

struct AuthOutcome {
    result: AuthResult,
    session_id: Option<u64>,
    authed_id: Option<Arc<str>>,
    role: Option<AuthedForRole>,
}

impl ProtocolHandler for CentralRpcServer {
    #[instrument(name = "irpc_conn", skip_all, fields(remote = tracing::field::Empty))]
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let remote = conn.remote_id();
        tracing::Span::current().record("remote", tracing::field::display(remote.fmt_short()));
        debug!("irpc connection accepted");

        let mut session: Option<(u64, Arc<str>, AuthedForRole)> = None;

        loop {
            let Some(msg) = irpc_iroh::read_request::<CentralProtocol>(&conn).await? else {
                break;
            };

            match msg {
                CentralRequest::Auth(req) => {
                    if session.is_some() {
                        conn.close(1u32.into(), b"already authenticated");
                        break;
                    }
                    let WithChannels { inner, tx, .. } = req;
                    let outcome = self
                        .handle_auth(inner.api_key, inner.capabilities, inner.version, &conn)
                        .await;
                    let _ = tx.send(outcome.result).await;
                    match (outcome.session_id, outcome.authed_id, outcome.role) {
                        (Some(id), Some(authed), Some(role)) => {
                            info!(?authed, role = %role, "irpc connection authenticated");
                            session = Some((id, authed, role));
                        }
                        _ => {
                            conn.close(1u32.into(), b"unauthorized");
                            break;
                        }
                    }
                }
                req => {
                    let Some((session_id, authed_id, role)) = session.clone() else {
                        conn.close(1u32.into(), b"unauthenticated");
                        break;
                    };
                    let server = self.clone();
                    let conn = conn.clone();
                    tokio::spawn(async move {
                        server
                            .dispatch(req, session_id, authed_id, role, conn)
                            .await;
                    });
                }
            }
        }

        if let Some((id, authed, _)) = session {
            distributor().disconnect(id).await;
            self.registry.unregister(id);
            if let Err(e) = Database::global()
                .connections_remove(central_id(), authed)
                .await
            {
                warn!(?e, "Failed to remove connection inventory row");
            }
        }
        let _ = conn.closed().await;
        Ok(())
    }
}

impl CentralRpcServer {
    async fn handle_auth(
        &self,
        api_key: Arc<str>,
        capabilities: Capabilities,
        version: String,
        conn: &Connection,
    ) -> AuthOutcome {
        match Database::global().authed_get_info_by_token(api_key).await {
            Ok(AuthedInfoResponse::Authorized(info)) => {
                let caps_json = serde_json::to_string(&capabilities).ok();
                let role: &'static str = (&info.for_role).into();
                if let Err(e) = Database::global()
                    .connections_upsert(
                        central_id(),
                        info.id.clone(),
                        role,
                        caps_json,
                        Some(version.clone()),
                    )
                    .await
                {
                    warn!(?e, "Failed to upsert connection inventory row");
                }

                let id = self
                    .registry
                    .register(info.id.clone(), conn.clone(), info.expires_at);
                metrics::auth_ok();
                AuthOutcome {
                    result: AuthResult::Ok,
                    session_id: Some(id),
                    authed_id: Some(info.id),
                    role: Some(info.for_role),
                }
            }
            Ok(AuthedInfoResponse::NotAuthorized { error }) => {
                debug!(%error, "irpc auth rejected");
                metrics::auth_unauthorized();
                AuthOutcome {
                    result: AuthResult::Unauthorized,
                    session_id: None,
                    authed_id: None,
                    role: None,
                }
            }
            Err(e) => {
                error!(?e, "irpc auth DB lookup failed");
                metrics::auth_unauthorized();
                AuthOutcome {
                    result: AuthResult::Unauthorized,
                    session_id: None,
                    authed_id: None,
                    role: None,
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn dispatch(
        &self,
        req: CentralRequest,
        session_id: u64,
        authed_id: Arc<str>,
        role: AuthedForRole,
        conn: Connection,
    ) {
        let is_worker = matches!(role, AuthedForRole::Worker);
        let is_bot = matches!(role, AuthedForRole::Bot);
        metrics::rpc_request();
        match req {
            CentralRequest::Auth(_) => unreachable!("Auth is handled in the accept loop"),
            CentralRequest::Heartbeat(r) => {
                let WithChannels { tx, .. } = r;
                if let Err(e) = Database::global()
                    .connections_heartbeat(central_id(), authed_id)
                    .await
                {
                    warn!(?e, "Failed to touch connection inventory row");
                }
                let _ = tx.send(()).await;
            }
            CentralRequest::GetWorkItem(r) => {
                let WithChannels { tx, .. } = r;
                if !is_worker {
                    let _ = tx.send(GetWorkItemResult::Unauthorized).await;
                    return;
                }
                distributor().park(authed_id, session_id, tx).await;
            }
            CentralRequest::RefuseWorkItem(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx.send(FreeResult::Unauthorized { request_id }).await;
                    return;
                }
                match Database::global()
                    .requests_refuse(request_id.clone(), authed_id)
                    .await
                {
                    Ok(db_result) => {
                        let wire: FreeResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "refuse work request failed");
                        let _ = tx.send(FreeResult::BackendError { request_id }).await;
                    }
                }
            }
            CentralRequest::WorkRequestFree(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx.send(FreeResult::Unauthorized { request_id }).await;
                    return;
                }
                match Database::global()
                    .requests_free(request_id.clone(), authed_id)
                    .await
                {
                    Ok(db_result) => {
                        let wire: FreeResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "free work request failed");
                        let _ = tx.send(FreeResult::BackendError { request_id }).await;
                    }
                }
            }
            CentralRequest::WorkRequestUpdateStatus(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx
                        .send(UpdateStatusMessageResult {
                            request_id,
                            result: UpdateStatusMessageResultStatus::Unauthorized,
                        })
                        .await;
                    return;
                }
                match Database::global()
                    .requests_update_status_message(
                        request_id.clone(),
                        authed_id,
                        inner.message.as_ref(),
                    )
                    .await
                {
                    Ok(db_result) => {
                        let wire: UpdateStatusMessageResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "update status message failed");
                        let _ = tx
                            .send(UpdateStatusMessageResult {
                                request_id,
                                result: UpdateStatusMessageResultStatus::BackendError,
                            })
                            .await;
                    }
                }
            }
            CentralRequest::WorkRequestAddErrors(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx
                        .send(AddErrorsResult {
                            request_id,
                            result: AddErrorsResultStatus::Unauthorized,
                        })
                        .await;
                    return;
                }
                match Database::global()
                    .requests_add_errors(request_id.clone(), authed_id, inner.errors)
                    .await
                {
                    Ok(db_result) => {
                        let wire: AddErrorsResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "add errors failed");
                        let _ = tx
                            .send(AddErrorsResult {
                                request_id,
                                result: AddErrorsResultStatus::BackendError,
                            })
                            .await;
                    }
                }
            }
            CentralRequest::WorkRequestMoveToWaiting(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx
                        .send(MoveToWaitingForRequesterResult {
                            request_id,
                            result: MoveToWaitingForRequesterResultStatus::Unauthorized,
                        })
                        .await;
                    return;
                }
                let files_data = inner
                    .files_data
                    .into_iter()
                    .map(std::convert::Into::into)
                    .collect();
                match Database::global()
                    .requests_move_to_waiting_for_requester(
                        request_id.clone(),
                        authed_id,
                        files_data,
                    )
                    .await
                {
                    Ok(db_result) => {
                        let wire: MoveToWaitingForRequesterResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "move to waiting failed");
                        let _ = tx
                            .send(MoveToWaitingForRequesterResult {
                                request_id,
                                result: MoveToWaitingForRequesterResultStatus::BackendError,
                            })
                            .await;
                    }
                }
            }
            CentralRequest::WorkRequestFail(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_worker {
                    let _ = tx
                        .send(FailResult {
                            request_id,
                            result: FailResultStatus::Unauthorized,
                        })
                        .await;
                    return;
                }
                match Database::global()
                    .requests_fail(request_id.clone(), authed_id, inner.reason.as_ref())
                    .await
                {
                    Ok(db_result) => {
                        let wire: FailResult = (request_id, db_result).into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "fail work request failed");
                        let _ = tx
                            .send(FailResult {
                                request_id,
                                result: FailResultStatus::BackendError,
                            })
                            .await;
                    }
                }
            }
            CentralRequest::WorkRequestMake(r) => {
                let WithChannels { inner, tx, .. } = r;
                if !is_bot {
                    let _ = tx.send(CreateResult::Unauthorized).await;
                    return;
                }
                match Database::global()
                    .requests_add(authed_id, inner.info, inner.metadata, inner.idempotency_key)
                    .await
                {
                    Ok(resp) => {
                        let wire = CreateResult::Ok(CreateResultData { id: resp.id });
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "create work request failed");
                        let _ = tx.send(CreateResult::BackendError).await;
                    }
                }
            }
            CentralRequest::WorkRequestComplete(r) => {
                let WithChannels { inner, tx, .. } = r;
                let request_id = inner.request_id.clone();
                if !is_bot {
                    let _ = tx.send(FinishResult::Unauthorized).await;
                    return;
                }
                match Database::global()
                    .requests_finish(request_id, authed_id)
                    .await
                {
                    Ok(db_result) => {
                        let wire: FinishResult = db_result.into();
                        let _ = tx.send(wire).await;
                    }
                    Err(e) => {
                        error!(?e, "complete work request failed");
                        let _ = tx.send(FinishResult::BackendError).await;
                    }
                }
            }
            CentralRequest::WorkRequestGetMineInProgress(r) => {
                let WithChannels { tx, .. } = r;
                if !is_bot {
                    return;
                }
                spawn_watch_mine_in_progress(tx, authed_id, conn);
            }
            CentralRequest::GetCapabilities(r) => {
                let WithChannels { tx, .. } = r;
                let summary = aggregate_capabilities().await;
                let _ = tx.send(summary).await;
            }
        }
    }
}

async fn aggregate_capabilities() -> CapabilitiesSummary {
    use std::collections::HashSet;

    use app_peer_comms::rpc::request::HandlerEntry;

    let rows = match Database::global().connections_list().await {
        Ok(rows) => rows,
        Err(e) => {
            warn!(?e, "connections_list failed for capabilities aggregate");
            return CapabilitiesSummary::default();
        }
    };

    let mut extractors: Vec<HandlerEntry> = Vec::new();
    let mut downloaders: Vec<HandlerEntry> = Vec::new();
    let mut fixers: Vec<HandlerEntry> = Vec::new();
    let mut seen_x = HashSet::new();
    let mut seen_d = HashSet::new();
    let mut seen_f = HashSet::new();

    for row in rows {
        if row.role != "worker" {
            continue;
        }
        let Some(json) = row.capabilities else {
            continue;
        };
        let Ok(Capabilities::Worker {
            extractors: ex,
            downloaders: dl,
            fixers: fx,
        }) = serde_json::from_str::<Capabilities>(&json)
        else {
            continue;
        };
        for e in ex {
            if seen_x.insert(e.name.clone()) {
                extractors.push(e);
            }
        }
        for d in dl {
            if seen_d.insert(d.name.clone()) {
                downloaders.push(d);
            }
        }
        for f in fx {
            if seen_f.insert(f.name.clone()) {
                fixers.push(f);
            }
        }
    }

    CapabilitiesSummary {
        extractors,
        downloaders,
        fixers,
    }
}

fn spawn_watch_mine_in_progress(
    tx: app_peer_comms::irpc::channel::mpsc::Sender<WorkRequestSnapshot>,
    authed_id: Arc<str>,
    conn: Connection,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let stream = match Database::global()
            .requests_watch_mine_in_progress(authed_id)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                error!(?e, "watch mine in progress subscribe failed");
                return;
            }
        };

        tokio::pin!(stream);
        loop {
            tokio::select! {
                biased;
                closed = conn.closed() => {
                    debug!(?closed, "Watch client disconnected; ending stream");
                    return;
                }
                emission = stream.next() => {
                    let Some(emission) = emission else { return; };
                    match emission {
                        Ok(list) => {
                            let mut requests = Vec::with_capacity(list.len());
                            for item in list.iter() {
                                match std::convert::TryInto::<WorkRequest>::try_into(item) {
                                    Ok(wr) => requests.push(wr),
                                    Err(e) => warn!(?e, "work request convert failed"),
                                }
                            }
                            let snapshot = WorkRequestSnapshot {
                                requests: requests.into(),
                            };
                            if tx.send(snapshot).await.is_err() {
                                return;
                            }
                        }
                        Err(e) => warn!(?e, "watch mine in progress emission error"),
                    }
                }
            }
        }
    })
}
