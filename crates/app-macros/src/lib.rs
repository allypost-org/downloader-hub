use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(Dumpable)]
pub fn dumpable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl ::app_config::Dumpable for #name {
            fn config_for_dump(&self) -> Option<Option<::app_config::DumpConfigType>> {
                self.dump.dump_config.clone()
            }
        }

        #[derive(
            Debug,
            Clone,
            Default,
            ::serde::Serialize,
            ::serde::Deserialize,
            ::clap::Args,
            ::validator::Validate,
        )]
        #[allow(clippy::option_option)]
        #[clap(next_help_heading = Some("Dump options"))]
        pub struct DumpConfig {
            /// Dump the config to stdout
            #[arg(long, value_enum, default_value = None, value_name = "TYPE")]
            pub dump_config: Option<Option<::app_config::DumpConfigType>>,

            /// Dump shell completions to stdout
            #[arg(long, default_value = None, value_name = "SHELL", value_parser = #name::hacky_dump_completions())]
            #[serde(skip)]
            pub dump_completions: Option<::app_config::Shell>,
        }
    };
    TokenStream::from(expanded)
}
