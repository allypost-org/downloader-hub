use clap::{CommandFactory, ValueEnum, builder::TypedValueParser};
pub use clap_complete::Shell;
use serde::{Deserialize, Serialize};
use validator::ValidationError;

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
pub enum DumpConfigType {
    Json,
    Toml,
}
pub trait Dumpable: Serialize + CommandFactory {
    #[allow(clippy::option_option)]
    fn config_for_dump(&self) -> Option<Option<DumpConfigType>>;

    #[must_use]
    fn dump_if_needed(self) -> Self {
        if let Some(dump_type) = self.config_for_dump() {
            let out = match dump_type {
                None | Some(DumpConfigType::Json) => {
                    serde_json::to_string_pretty(&self).expect("Failed to serialize config to JSON")
                }

                Some(DumpConfigType::Toml) => {
                    toml::to_string_pretty(&self).expect("Failed to serialize config to TOML")
                }
            };

            println!("{}", out.trim());
            std::process::exit(0);
        }

        self
    }

    #[must_use]
    fn hacky_dump_completions() -> impl TypedValueParser {
        move |s: &str| {
            let parsed = <Shell as ValueEnum>::from_str(s, true);

            if let Ok(shell) = &parsed {
                let bin_name = std::env::current_exe()
                    .map_err(|_e| ValidationError::new("Unknown application name"))?
                    .file_name()
                    .map(|x| x.to_string_lossy().to_string())
                    .ok_or_else(|| ValidationError::new("Unknown application name"))?;

                clap_complete::generate(
                    *shell,
                    &mut Self::command(),
                    bin_name,
                    &mut std::io::stdout(),
                );
                std::process::exit(0);
            }

            parsed
                .map(|_| ())
                .map_err(|_| ValidationError::new("Invalid shell"))
        }
    }
}
