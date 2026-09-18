use clap::Parser;
use validator::Validate;

use crate::{Dumpable, validators::print_validation_errors};

pub trait BootConfig: Parser + Validate + Dumpable + Sized {
    #[must_use]
    fn resolve_paths(self) -> Self {
        self
    }

    #[must_use]
    fn parse_validated() -> Self {
        let parsed = Self::parse().resolve_paths();

        if let Err(e) = parsed.validate() {
            eprintln!("Errors validating configuration:");
            print_validation_errors(&e, "  ", 1);
            std::process::exit(1);
        }

        parsed
    }

    fn init_parsed() -> Result<Self, String> {
        Ok(Self::parse_validated().dump_if_needed())
    }
}
