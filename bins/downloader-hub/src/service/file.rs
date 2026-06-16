use std::{io::Read, path::Path};

use app_helpers::{encoding::to_base64, file_type::infer_file_type};
use sha2::Digest;

pub struct FileService;

impl FileService {
    pub async fn file_hash(file: &Path) -> anyhow::Result<String> {
        let f = file.to_path_buf();

        tokio::task::spawn_blocking(|| -> anyhow::Result<String> {
            let input = std::fs::File::open(f)?;
            let mut reader = std::io::BufReader::new(input);
            let mut hasher = sha2::Sha384::new();

            let mut buffer = [0u8; 8192];
            loop {
                let n = reader
                    .read(&mut buffer)
                    .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }

            Ok(format!(
                "sha384:{digest}",
                digest = to_base64(hasher.finalize())
            ))
        })
        .await?
    }

    pub async fn infer_file_type(file: &Path) -> anyhow::Result<String> {
        let file = file.to_path_buf();
        tokio::task::spawn_blocking(move || infer_file_type(&file))
            .await?
            .map(|x| x.to_string())
    }
}
