use app_config::{
    Dumpable, GlobalConfig, common, conditional::telegram_bot::TelegramBotConfig as BotConfig,
    validators::print_validation_errors,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Validate, GlobalConfig, Dumpable)]
pub struct Config {
    #[clap(flatten)]
    #[validate(nested)]
    pub bot: BotConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub dependency_paths: common::ProgramPathConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub disabled_entries: common::DisabledEntriesConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub endpoint: common::EndpointConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub task: common::TaskConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub request: common::RequestConfig,

    #[clap(flatten)]
    #[validate(nested)]
    #[serde(skip)]
    dump: DumpConfig,
}

impl Config {
    pub fn init_parsed() -> Result<&'static Self, String> {
        let parsed = Self::parse()
            .resolve_paths()
            .validate_or_exit()
            .dump_if_needed();

        {
            let parsed = parsed.clone();
            app_actions::config::init(
                parsed.endpoint,
                parsed.dependency_paths,
                parsed.disabled_entries.entries,
                parsed.request,
            )?;
        }

        {
            let parsed = parsed.clone();
            app_helpers::config::init(parsed.dependency_paths)?;
        }

        {
            let parsed = parsed.clone();
            app_tasks::config::init(parsed.task)?;
        }

        Self::init(parsed)
    }

    #[must_use]
    #[inline]
    pub fn bot() -> &'static BotConfig {
        &Self::global().bot
    }

    #[inline]
    fn resolve_paths(mut self) -> Self {
        self.dependency_paths = self.dependency_paths.resolve_paths();
        self
    }

    #[inline]
    fn validate_or_exit(self) -> Self {
        if let Err(e) = self.validate() {
            eprintln!("Errors validating configuration:");
            print_validation_errors(&e, "  ", 1);
            std::process::exit(1);
        }

        self
    }
}
