use std::convert::Into;

use app_helpers::file_time::transferable_file_times;
pub use common::{FixRequest, FixResult, FixerError, FixerReturn};
pub use handlers::AVAILABLE_FIXERS;
use handlers::FixerInstance;
use tracing::{Instrument, debug, trace, warn};

use crate::config::ActionsConfig;

mod common;
pub mod handlers;

#[async_trait::async_trait]
#[typetag::serde(tag = "$fixer")]
pub trait Fixer: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn description(&self) -> &'static str;

    fn can_run(&self) -> bool {
        true
    }

    fn is_enabled(&self) -> bool {
        ActionsConfig::global().is_enabled(("fixer", self.name()))
    }

    #[allow(unused_variables)]
    async fn can_run_for(&self, request: &FixRequest) -> bool {
        true
    }

    async fn run(&self, request: &FixRequest) -> FixerReturn;
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

pub async fn fix_file(request: FixRequest) -> FixerReturn {
    fix_file_with(AVAILABLE_FIXERS.clone(), request).await
}

#[tracing::instrument(skip(request))]
pub async fn fix_file_with(fixers: Vec<FixerInstance>, request: FixRequest) -> FixerReturn {
    let request = request.resolve_path()?.check_path()?;
    debug!(?request, "Fixing file");

    let transfer_file_times = transferable_file_times(&request.file_path);

    let mut req = request.clone();
    for fixer in fixers {
        trace!(?fixer, "Trying fixer");

        if !fixer.can_run_for(&req).await {
            continue;
        }

        trace!("Running fixer {fixer:?} on {req:?}");

        let result = match fixer
            .run(&req)
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
