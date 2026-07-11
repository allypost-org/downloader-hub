use std::{path::PathBuf, sync::Arc};

pub mod impls;

pub trait Downloadable {
    type Error;

    #[allow(dead_code)]
    fn get_suggested_name(&self) -> Option<Arc<str>> {
        None
    }

    async fn download_into(
        &self,
        to: tokio::fs::File,
    ) -> Result<(tokio::fs::File, Option<PathBuf>), Self::Error>;
}
