pub mod api;
pub mod client;
pub mod entity;
pub mod error;
pub mod helpers;

pub use api::request::DatabaseRequest;
pub use client::Database;
pub use error::DatabaseError;
pub use futures::StreamExt;
pub use maplit::btreemap;

#[macro_export(local_inner_macros)]
macro_rules! arg_map {
    // trailing comma case
    ($($key:expr => $value:expr,)+) => (btreemap!($($key.into() => $value.into()),+));

    ( $($key:expr => $value:expr),* ) => {
        {
            let mut _map = ::std::collections::BTreeMap::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}
