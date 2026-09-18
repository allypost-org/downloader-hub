use std::{collections::HashSet, path::Path, sync::Arc};

use app_config::{AsEntryId, EntryId};
use futures::{StreamExt, stream::FuturesUnordered};
use tracing::{Instrument, trace};

use crate::{
    ActionCtx,
    actions::handlers::{ALL_ACTIONS, ActionEntry},
    downloaders::{DownloadRequest, DownloaderEntry, DownloaderReturn, handlers::ALL_DOWNLOADERS},
    extractors::{
        ExtractInfoRequest, ExtractedInfo,
        handlers::{ALL_EXTRACTORS, ExtractorEntry},
    },
    fixers::{
        FixRequest,
        handlers::{ALL_FIXERS, FixerInstance},
    },
};

#[derive(Debug, Clone)]
pub struct Actions {
    ctx: Arc<ActionCtx>,
    disabled: HashSet<EntryId>,
}

impl Actions {
    #[must_use]
    pub fn new(ctx: Arc<ActionCtx>, disabled_entries: Vec<EntryId>) -> Self {
        Self {
            ctx,
            disabled: disabled_entries.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn ctx(&self) -> &ActionCtx {
        &self.ctx
    }

    #[must_use]
    pub fn is_enabled(&self, id: impl AsEntryId) -> bool {
        !self.disabled.contains(&id.entry_id())
    }

    pub fn all_actions(&self) -> impl Iterator<Item = &ActionEntry> {
        ALL_ACTIONS.iter()
    }

    pub fn all_downloaders(&self) -> impl Iterator<Item = &DownloaderEntry> {
        ALL_DOWNLOADERS.iter()
    }

    pub fn all_fixers(&self) -> impl Iterator<Item = &FixerInstance> {
        ALL_FIXERS.iter()
    }

    pub fn all_extractors(&self) -> impl Iterator<Item = &ExtractorEntry> {
        ALL_EXTRACTORS.iter()
    }

    pub async fn enabled_actions(&self) -> Vec<ActionEntry> {
        let mut enabled = Vec::new();
        for action in ALL_ACTIONS.iter() {
            if !self.is_enabled(action.as_ref()) {
                trace!(?action, "Action is disabled");
                continue;
            }
            if action.can_run(&self.ctx).await {
                enabled.push(action.clone());
            }
        }
        enabled
    }

    #[must_use]
    pub fn enabled_downloaders(&self) -> Vec<DownloaderEntry> {
        ALL_DOWNLOADERS
            .iter()
            .filter(|x| self.is_enabled(x.as_ref()))
            .cloned()
            .collect()
    }

    pub async fn enabled_fixers(&self) -> Vec<FixerInstance> {
        let mut enabled = Vec::new();
        for fixer in ALL_FIXERS.iter() {
            if !self.is_enabled(fixer.as_ref()) {
                trace!(?fixer, "Fixer is disabled");
                continue;
            }
            if fixer.can_run(&self.ctx).await {
                enabled.push(fixer.clone());
            }
        }
        enabled
    }

    pub fn enabled_extractors(&self) -> impl Iterator<Item = &ExtractorEntry> {
        ALL_EXTRACTORS
            .iter()
            .filter(|x| self.is_enabled(x.as_ref()))
    }

    pub async fn first_available_extractor(
        &self,
        request: &ExtractInfoRequest,
    ) -> Option<ExtractorEntry> {
        for extractor in ALL_EXTRACTORS.iter() {
            if self.is_enabled(extractor.as_ref()) && extractor.can_handle(request).await {
                return Some(extractor.clone());
            }
        }
        None
    }

    pub async fn extract_info(
        &self,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        let extractor = self
            .first_available_extractor(request)
            .await
            .ok_or_else(|| "No extractor found".to_string())?;

        trace!(?extractor, "Found extractor");

        crate::extractors::extract_info_with(&self.ctx, request, extractor.clone()).await
    }

    pub async fn download_file<R>(&self, request: R, download_dir: &Path) -> Vec<DownloaderReturn>
    where
        R: Into<ExtractInfoRequest> + Send + Sync + std::fmt::Debug,
    {
        let request = request.into();

        tracing::debug!(?request, "Extracting info");

        let s =
            tracing::info_span!("download_file", request = ?request, download_dir = ?download_dir);

        async move {
            let info = match self.extract_info(&request).await {
                Ok(x) => x,
                Err(e) => {
                    return vec![Err(crate::downloaders::DownloaderError::FallibleFailed(
                        format!("Failed to extract info from {request:?}: <u>{e}</u>"),
                    ))];
                }
            };

            tracing::debug!(?info, "Extracted info");

            let download_requests = DownloadRequest::from_extracted_info(&info, download_dir);

            tracing::debug!(?download_requests, "Download requests");

            let downloaders = self.enabled_downloaders();
            let results = download_requests
                .into_iter()
                .map(|req| {
                    let downloaders = downloaders.clone();
                    async move {
                        crate::downloaders::download_file_with(&self.ctx, &downloaders, &req).await
                    }
                })
                .collect::<FuturesUnordered<_>>()
                .collect::<Vec<_>>()
                .await;

            tracing::debug!(?results, "Download results");
            results
        }
        .instrument(s)
        .await
    }

    #[tracing::instrument(name = "fix_file", skip_all, fields(request = ?request))]
    pub async fn fix_file<R>(&self, request: R) -> crate::fixers::FixerReturn
    where
        R: Into<FixRequest> + Send + Sync + std::fmt::Debug,
    {
        let fixers = self.enabled_fixers().await;
        crate::fixers::fix_file_with(&self.ctx, fixers, request.into()).await
    }
}
