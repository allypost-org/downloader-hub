use std::sync::{Arc, Mutex, OnceLock};

use app_database::{
    Database,
    api::{
        log_settings::{LogSettings, LogSettingsScope, resolve_log_settings},
        secrets::SecretEntry,
    },
    entity::authed::AuthedForRole,
};
use app_peer_comms::PeeringEndpoint;
use arc_swap::ArcSwapOption;
use tokio::{sync::RwLock, task::JoinHandle};

use super::rpc::{RestrictionsManager, SessionRegistry, WorkDistributor};

#[derive(Default)]
pub struct CentralState {
    pub db: OnceLock<Arc<Database>>,
    pub sessions: SessionRegistry,
    pub distributor: ArcSwapOption<WorkDistributor>,
    initial_distributor_handle: Mutex<Option<JoinHandle<()>>>,
    pub restrictions: ArcSwapOption<RestrictionsManager>,
    pub log_settings: RwLock<Vec<LogSettings>>,
    pub secrets: RwLock<Vec<SecretEntry>>,
    pub peering: OnceLock<Arc<PeeringEndpoint>>,
}

pub type SharedCentralState = Arc<CentralState>;

impl std::fmt::Debug for CentralState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CentralState").finish_non_exhaustive()
    }
}

impl CentralState {
    #[must_use]
    pub fn shared() -> SharedCentralState {
        Arc::new(Self::default())
    }

    pub fn db(&self) -> &Arc<Database> {
        self.db.get().expect("central database not initialized")
    }

    pub fn set_distributor(&self, handle: WorkDistributor, join: JoinHandle<()>) {
        self.distributor.store(Some(Arc::new(handle)));
        *self
            .initial_distributor_handle
            .lock()
            .expect("distributor handle lock poisoned") = Some(join);
    }

    pub fn distributor(&self) -> Arc<WorkDistributor> {
        self.distributor
            .load_full()
            .expect("work distributor not initialized")
    }

    pub fn take_initial_distributor_handle(&self) -> Option<JoinHandle<()>> {
        self.initial_distributor_handle
            .lock()
            .expect("distributor handle lock poisoned")
            .take()
    }

    pub fn respawn_distributor(&self) -> JoinHandle<()> {
        let (handle, join) = WorkDistributor::spawn(self.db().clone());
        self.distributor.store(Some(Arc::new(handle)));
        join
    }

    pub async fn set_log_settings(&self, settings: Vec<LogSettings>) {
        *self.log_settings.write().await = settings;
    }

    pub async fn set_secrets(&self, secrets: Vec<SecretEntry>) {
        *self.secrets.write().await = secrets;
    }

    pub async fn log_settings_for(
        &self,
        role: &AuthedForRole,
    ) -> Option<app_peer_comms::rpc::request::LogSettings> {
        let scope = match role {
            AuthedForRole::Worker => LogSettingsScope::Worker,
            AuthedForRole::Bot => LogSettingsScope::Bot,
            AuthedForRole::Admin => return None,
        };
        let settings = self.log_settings.read().await;
        let effective = resolve_log_settings(&settings, scope);
        drop(settings);
        Some(app_peer_comms::rpc::request::LogSettings {
            console: effective.console,
            file: effective.file,
        })
    }
}
