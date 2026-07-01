use std::sync::LazyLock;

use app_peer_comms::message::v1::{
    central::work_request::{WorkRequest, WorkRequestInfo},
    common::file::FileReference,
};
use moka::future::Cache as MokaCache;
use tracing::{debug, error, info, instrument, trace};

use crate::cmd::work::app::{
    IS_PROCESSING, broadcaster::Broadcaster,
    helpers::extract_info_request::file_url_to_extract_info_request,
};

pub mod take;

pub static RECENTLY_HANDLED: LazyLock<MokaCache<String, bool>> = LazyLock::new(|| {
    moka::future::CacheBuilder::new(100)
        .time_to_live(std::time::Duration::from_hours(1))
        .build()
});

#[instrument(name = "work-request", skip_all, fields(id = %request.request_id))]
pub async fn handle_work_request(
    request: &WorkRequest,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if IS_PROCESSING.available_permits() == 0 {
        debug!("Some task already in progress, not taking request");
        return Ok(false);
    }

    if RECENTLY_HANDLED
        .get(request.request_id.as_ref())
        .await
        .is_some()
    {
        debug!("Request recently handled, not taking request");
        return Ok(false);
    }

    match &request.info {
        WorkRequestInfo::DownloadAndFix(file_ref) => match file_ref {
            FileReference::Url(url) => {
                let req = file_url_to_extract_info_request(url)?;
                trace!(?req, "Extracting info");
                let extractor = req.first_available_extractor().await;
                let Some(extractor) = extractor else {
                    return Ok(false);
                };

                debug!(?extractor, "Found extractor");

                if !extractor.is_enabled() {
                    info!("Matched extractor is disabled, not taking request");
                    return Ok(false);
                }

                info!("Taking request");

                Broadcaster::get().send_work_request_take(request.request_id.clone());

                RECENTLY_HANDLED
                    .insert(request.request_id.to_string(), true)
                    .await;

                return Ok(true);
            }
            FileReference::BlobTicket(ticket) => {
                error!(?ticket, "Blob tickets not supported yet!");
                return Ok(false);
            }
        },
    }
}
