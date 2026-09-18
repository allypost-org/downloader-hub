use app_config::{EntryCategory, EntryId};
pub use common::{
    extract_info_request::ExtractInfoRequest,
    extracted_info::{ExtractedInfo, ExtractedUrlInfo},
};
pub use handlers::ALL_EXTRACTORS;
use tracing::trace;

mod common;
pub mod handlers;

#[async_trait::async_trait]
#[typetag::serde(tag = "$extractor")]
pub trait Extractor: std::fmt::Debug {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn entry_id(&self) -> EntryId {
        EntryId::new(EntryCategory::Extractor, self.name())
    }

    fn description(&self) -> &'static str;

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool;

    async fn extract_info(
        &self,
        ctx: &crate::ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String>;
}

impl app_config::AsEntryId for dyn Extractor + Sync + Send {
    fn entry_id(&self) -> EntryId {
        Extractor::entry_id(self)
    }
}

#[tracing::instrument(skip_all, fields(extractor = %extractor.name()))]
pub async fn extract_info_with(
    ctx: &crate::ActionCtx,
    request: &ExtractInfoRequest,
    extractor: handlers::ExtractorEntry,
) -> Result<ExtractedInfo, String> {
    trace!("Extracting info");

    let info = extractor
        .extract_info(ctx, request)
        .await?
        .with_meta(
            "extractor",
            serde_json::to_value(extractor).expect("Failed to serialize extractor"),
        )
        .dedup_urls();

    Ok(info)
}
