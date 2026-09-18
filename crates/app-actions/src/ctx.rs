use std::sync::Arc;

use app_config::common::{EndpointConfig, ProgramPathConfig, RequestConfig};

#[derive(Debug, Clone)]
pub struct ActionCtx {
    pub endpoint: Arc<EndpointConfig>,
    pub dependency_paths: Arc<ProgramPathConfig>,
    pub request: Arc<RequestConfig>,
}

impl ActionCtx {
    #[must_use]
    pub fn new(
        endpoint: EndpointConfig,
        dependency_paths: ProgramPathConfig,
        request: RequestConfig,
    ) -> Self {
        Self {
            endpoint: Arc::new(endpoint),
            dependency_paths: Arc::new(dependency_paths),
            request: Arc::new(request),
        }
    }
}
