mod common;
pub mod handlers;

use std::fmt::Debug;

use app_config::{EntryCategory, EntryId};
pub use common::{
    action_error::ActionError,
    action_request::{ActionOptions, ActionRequest},
    action_result::{ActionResult, ActionResultData},
};
pub use handlers::ALL_ACTIONS;

#[async_trait::async_trait]
#[typetag::serde(tag = "$action")]
pub trait Action: Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn entry_id(&self) -> EntryId {
        EntryId::new(EntryCategory::Action, self.name())
    }

    fn description(&self) -> &'static str;

    async fn can_run(&self, _ctx: &crate::ActionCtx) -> bool {
        true
    }

    async fn can_run_for(&self, _req: &ActionRequest) -> bool {
        true
    }

    async fn run(
        &self,
        ctx: &crate::ActionCtx,
        req: &ActionRequest,
    ) -> Result<ActionResult, ActionError>;
}

impl app_config::AsEntryId for dyn Action {
    fn entry_id(&self) -> EntryId {
        Action::entry_id(self)
    }
}
