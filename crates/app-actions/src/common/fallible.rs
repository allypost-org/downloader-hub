use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum TaskFallible {
    CanFail,
    #[default]
    MustSucceed,
}

impl TaskFallible {
    #[must_use]
    pub const fn can_fail(self) -> bool {
        matches!(self, Self::CanFail)
    }
}
