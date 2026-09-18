use serde::{Deserialize, Serialize};

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::ActionCtx;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Fallthrough;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Fallthrough {
    fn description(&self) -> &'static str {
        "Fallthrough extractor. Does nothing, just forwards the URL."
    }

    async fn can_handle(&self, _request: &ExtractInfoRequest) -> bool {
        true
    }

    async fn extract_info(
        &self,
        _ctx: &ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        Ok(ExtractedInfo::from_url(request, request.url.as_str()))
    }
}
