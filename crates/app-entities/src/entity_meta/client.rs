use std::path::PathBuf;

use serde::Serialize;

use super::common::path::AppPath;
use crate::client;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientWithHidden {
    pub id: i32,
    #[serde(flatten)]
    pub client: client::Model,
    pub app_meta: serde_json::Value,
}
impl From<client::Model> for ClientWithHidden {
    fn from(value: client::Model) -> Self {
        Self {
            id: value.id,
            app_meta: value.app_meta.clone(),
            client: value,
        }
    }
}

impl client::Model {
    pub fn resolve_download_folder(&self) -> anyhow::Result<PathBuf> {
        let download_dir = match AppPath::try_from(&self.download_folder) {
            Ok(AppPath::LocalAbsolute(path)) => path,
            _ => {
                return Err(anyhow::anyhow!(format!(
                    "Download folder is not a local path: {dir:?}",
                    dir = self.download_folder,
                )));
            }
        };

        if !download_dir.exists() {
            std::fs::create_dir_all(&download_dir)?;
        }

        if !download_dir.is_dir() {
            return Err(anyhow::anyhow!(
                "Download directory is not a directory: {download_dir:?}"
            ));
        }

        Ok(download_dir)
    }
}
