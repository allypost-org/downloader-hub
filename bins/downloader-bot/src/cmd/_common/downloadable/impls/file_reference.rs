use std::{path::PathBuf, str::FromStr, sync::Arc};

use anyhow::Context;
use app_peer_comms::message::v1::common::file::FileReference;
use futures::StreamExt;
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::debug;

use super::Downloadable;

impl Downloadable for FileReference {
    type Error = anyhow::Error;

    fn get_suggested_name(&self) -> Option<Arc<str>> {
        match self {
            Self::Url(_) => None,
            Self::BlobTicket(ticket) => Some(ticket.file_name.clone()),
        }
    }

    async fn download_into(&self, mut to: File) -> Result<(File, Option<PathBuf>), Self::Error> {
        debug!(from = ?self, "Downloading file");
        let res = match self {
            Self::Url(url) => {
                let headers = url
                    .headers
                    .iter()
                    .filter_map(|(k, v)| {
                        let k = k.parse().ok()?;
                        let v = v.parse().ok()?;
                        Some((k, v))
                    })
                    .collect();

                let mut resp = app_requests::Client::builder()
                    .build()?
                    .request(url.method.parse()?, url.url.clone())
                    .headers(headers)
                    .send()
                    .await
                    .context("Failed to connect to download endpoint")?
                    .error_for_status()
                    .context("Got error for status while downloading from endpoint")?
                    .bytes_stream();

                while let Some(chunk) = resp.next().await {
                    let chunk = match chunk {
                        Ok(x) => x,
                        Err(e) => {
                            debug!(?e, "Got error while downloading file");
                            return Err(
                                anyhow::anyhow!("Got error while downloading file: {e}").context(e)
                            );
                        }
                    };

                    to.write_all(&chunk)
                        .await
                        .context("Could not write to file while downloading")?;
                }

                Ok((to, None))
            }

            Self::BlobTicket(ticket) => app_peer_comms::PeeringEndpoint::download_ticket_into(
                ticket.ticket.clone(),
                &mut to,
            )
            .await
            .map(|_| (to, PathBuf::from_str(ticket.file_name.as_ref()).ok()))
            .map_err(|e| anyhow::anyhow!(e).context("Failed to download file via iroh ticket")),
        };

        debug!(?res, "Finished downloading file");

        res
    }
}
