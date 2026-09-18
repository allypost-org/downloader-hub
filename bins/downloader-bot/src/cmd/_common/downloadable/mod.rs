use std::{path::PathBuf, sync::Arc};

use app_peer_comms::PeeringEndpoint;

pub mod impls;

pub trait Downloadable {
    type Error;

    #[allow(dead_code)]
    fn get_suggested_name(&self) -> Option<Arc<str>> {
        None
    }

    async fn download_into(
        &self,
        peering: &PeeringEndpoint,
        to: tokio::fs::File,
    ) -> Result<(tokio::fs::File, Option<PathBuf>), Self::Error>;
}
