use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(GlobalConfig)]
pub fn global_config_derive(input: TokenStream) -> TokenStream {
    // Parse the struct
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl ::app_config::GlobalConfig for #name {
            fn global_instance() -> &'static ::std::sync::OnceLock<Self> {
                static GLOBAL: ::std::sync::OnceLock<#name> = ::std::sync::OnceLock::new();
                &GLOBAL
            }
        }

        impl #name {
            #[must_use]
            #[inline]
            pub fn global() -> &'static Self {
                Self::get_global()
                    .expect(&format!("Config not initialized for {}", stringify!(#name)))
            }

            #[must_use]
            #[inline]
            pub fn global_initialized() -> bool {
                Self::initialized_global()
            }

            #[must_use]
            #[inline]
            pub fn init(conf: Self) -> ::std::result::Result<&'static Self, String> {
                Self::init_global(conf)?;

                Ok(Self::global())
            }
        }
    };
    TokenStream::from(expanded)
}

#[proc_macro_derive(Dumpable)]
pub fn dumpable_derive(input: TokenStream) -> TokenStream {
    // Parse the struct
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
