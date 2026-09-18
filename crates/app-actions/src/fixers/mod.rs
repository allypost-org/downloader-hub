use std::convert::Into;

use app_config::{EntryCategory, EntryId};
use app_helpers::file_time::transferable_file_times;
pub use common::{FixRequest, FixResult, FixerError, FixerReturn};
pub use handlers::ALL_FIXERS;
use handlers::FixerInstance;
use tracing::{Instrument, debug, trace, warn};

mod common;
pub mod handlers;

#[async_trait::async_trait]
#[typetag::serde(tag = "$fixer")]
pub trait Fixer: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn entry_id(&self) -> EntryId {
        EntryId::new(EntryCategory::Fixer, self.name())
    }

    fn description(&self) -> &'static str;

    async fn can_run(&self, _ctx: &crate::ActionCtx) -> bool {
        true
    }

    #[allow(unused_variables)]
    async fn can_run_for(&self, ctx: &crate::ActionCtx, request: &FixRequest) -> bool {
        true
    }

    async fn run(&self, ctx: &crate::ActionCtx, request: &FixRequest) -> FixerReturn;
}

impl app_config::AsEntryId for dyn Fixer + Send + Sync {
    fn entry_id(&self) -> EntryId {
        Fixer::entry_id(self)
    }
}

pub trait IntoFixerReturn {
    fn into_fixer_return(self) -> FixerReturn;
}
impl<T, E> IntoFixerReturn for Result<T, E>
where
    T: Into<FixResult>,
    E: Into<FixerError>,
{
    fn into_fixer_return(self) -> FixerReturn {
        self.map(Into::into).map_err(Into::into)
    }
}

#[tracing::instrument(skip(ctx, request))]
pub async fn fix_file_with(
    ctx: &crate::ActionCtx,
    fixers: Vec<FixerInstance>,
    request: FixRequest,
) -> FixerReturn {
    let request = request.resolve_path()?.check_path()?;
    debug!(?request, "Fixing file");

    let transfer_file_times = transferable_file_times(&request.file_path);

    let mut req = request.clone();
    for fixer in fixers {
        trace!(?fixer, "Trying fixer");

        if !fixer.can_run_for(ctx, &req).await {
            continue;
        }

        trace!("Running fixer {fixer:?} on {req:?}");

        let result = match fixer
            .run(ctx, &req)
            .instrument(tracing::trace_span!("fixer", fixer = %fixer.name()))
            .await
        {
            Ok(x) => x,
            Err(e) => {
                warn!("Failed to run fixer {fixer:?} on {req:?}: {e:?}");
                continue;
            }
        };

        trace!(?result, "Fixer result");

        req = req.clone_with_path(result.file_path);
    }

    if let Ok(transfer_file_times) = transfer_file_times
        && req.file_path.as_os_str() != request.file_path.as_os_str()
        && let Err(e) = transfer_file_times(&req.file_path)
    {
        warn!("Failed to transfer file times of {request:?} to {req:?}: {e:?}");
    }

    debug!(?req, "Fixed file");

    Ok(FixResult::new(request.clone(), req.file_path))
}
