use std::sync::Arc;

pub use common::{
    extract_info_request::ExtractInfoRequest,
    extracted_info::{ExtractedInfo, ExtractedUrlInfo},
};
use futures::{FutureExt, StreamExt};
pub use handlers::AVAILABLE_EXTRACTORS;
use tracing::trace;

use crate::config::ActionsConfig;

mod common;
pub mod handlers;

#[async_trait::async_trait]
#[typetag::serde(tag = "$extractor")]
pub trait Extractor: std::fmt::Debug {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn description(&self) -> &'static str;

    fn is_enabled(&self) -> bool {
        ActionsConfig::global().is_enabled(("extractor", self.name()))
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool;

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String>;
}

impl ExtractInfoRequest {
    pub fn extractors(&self) -> impl futures::Stream<Item = (bool, handlers::ExtractorEntry)> {
        let req = Arc::new(self.clone());

        futures::stream::iter(AVAILABLE_EXTRACTORS.iter().cloned())
            .map(move |ex| {
                let req = req.clone();
                let ex = ex.clone();
                Box::pin(async move { (ex.can_handle(&req).await, ex) }.into_stream())
            })
            .flatten()
    }

    pub async fn first_available_extractor(&self) -> Option<handlers::ExtractorEntry> {
        let mut it = self.extractors();
        while let Some((can_handle, extractor)) = it.next().await {
            if can_handle {
                return Some(extractor);
            }
        }

        None
    }

    pub async fn extract_info(&self) -> Result<ExtractedInfo, String> {
        extract_info(self).await
    }

    pub async fn extract_info_with(
        &self,
        extractor: handlers::ExtractorEntry,
    ) -> Result<ExtractedInfo, String> {
        extract_info_with(self, extractor).await
    }
}

pub async fn extract_info(request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
    let extractor = request
        .first_available_extractor()
        .await
        .ok_or_else(|| "No extractor found".to_string())?;

    trace!(?extractor, "Found extractor");

    extract_info_with(request, extractor).await
}

#[tracing::instrument(skip_all, fields(extractor = %extractor.name()))]
pub async fn extract_info_with(
    request: &ExtractInfoRequest,
    extractor: handlers::ExtractorEntry,
) -> Result<ExtractedInfo, String> {
    trace!("Extracting info");

    let info = extractor
        .extract_info(request)
        .await?
        .with_meta(
            "extractor",
            serde_json::to_value(extractor).expect("Failed to serialize extractor"),
        )
        .dedup_urls();

    Ok(info)
}
