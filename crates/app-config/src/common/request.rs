use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const FALLBACK_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0";

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Request options"))]
pub struct RequestConfig {
    #[validate(length(min = 1))]
    #[arg(long, env = "USER_AGENT", default_value = FALLBACK_USER_AGENT)]
    pub user_agent: String,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            user_agent: FALLBACK_USER_AGENT.to_string(),
        }
    }
}
