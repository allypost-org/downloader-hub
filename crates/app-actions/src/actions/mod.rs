mod common;
pub mod handlers;

use std::fmt::Debug;

pub use common::{
    action_error::ActionError,
    action_request::{ActionOptions, ActionRequest},
    action_result::{ActionResult, ActionResultData},
};
pub use handlers::AVAILABLE_ACTIONS;

use crate::config::ActionsConfig;

#[async_trait::async_trait]
#[typetag::serde(tag = "$action")]
pub trait Action: Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn description(&self) -> &'static str;

    async fn can_run(&self) -> bool {
        true
    }

    fn is_enabled(&self) -> bool {
        ActionsConfig::global().is_enabled(("action", self.name()))
    }

    async fn can_run_for(&self, _req: &ActionRequest) -> bool {
        true
    }

    async fn run(&self, req: &ActionRequest) -> Result<ActionResult, ActionError>;
}
