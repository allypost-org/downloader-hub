use crate::cmd::central::{auth::ValidAuth, broadcaster::BroadcastAudience};

pub mod v1;

pub async fn handle_rpc(
    msg: app_peer_comms::Message,
    auth: ValidAuth,
    audiences: Vec<BroadcastAudience>,
) -> RpcReturn {
    match msg {
        app_peer_comms::Message::V1(v1) => v1::handle_v1_rpc(v1, auth, audiences).await,
    }
}

pub type RpcReturn = Result<Option<app_peer_comms::Message>, RpcError>;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("{0}")]
    Generic(String),

    #[error("{0}: {1}")]
    WithContext(String, Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Got database error: {0}")]
    Database(#[from] app_database::DatabaseError),
}

impl From<String> for RpcError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl<'a> From<&'a str> for RpcError {
    fn from(value: &'a str) -> Self {
        Self::Generic(value.into())
    }
}

impl<E> From<(String, E)> for RpcError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: (String, E)) -> Self {
        Self::WithContext(value.0, value.1.into())
    }
}

impl<'a, E> From<(&'a str, E)> for RpcError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: (&'a str, E)) -> Self {
        Self::WithContext(value.0.into(), value.1.into())
    }
}
