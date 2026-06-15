use std::collections::BTreeMap;

use tracing::trace;

use crate::{Database, DatabaseError, error::ResponseError};

#[derive(Debug)]
pub struct DatabaseRequest {
    pub name: String,
    pub args: BTreeMap<String, convex::Value>,
}

impl DatabaseRequest {
    #[must_use]
    pub const fn new(name: String, args: BTreeMap<String, convex::Value>) -> Self {
        Self { name, args }
    }

    #[must_use]
    pub fn named<T>(name: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            name: name.into(),
            args: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_arg<Key, Value>(mut self, key: Key, value: Value) -> Self
    where
        Key: Into<String>,
        Value: Into<convex::Value>,
    {
        self.with_arg_mut(key.into(), value.into());
        self
    }

    pub fn with_arg_mut<Key, Value>(&mut self, key: Key, value: Value) -> &mut Self
    where
        Key: Into<String>,
        Value: Into<convex::Value>,
    {
        self.args.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_args<Args, Key, Value>(mut self, args: Args) -> Self
    where
        Args: IntoIterator<Item = (Key, Value)>,
        Key: Into<String>,
        Value: Into<convex::Value>,
    {
        self.with_args_mut(args);
        self
    }

    pub fn with_args_mut<Args, Key, Value>(&mut self, args: Args) -> &mut Self
    where
        Args: IntoIterator<Item = (Key, Value)>,
        Key: Into<String>,
        Value: Into<convex::Value>,
    {
        self.args
            .extend(args.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    #[must_use]
    pub fn omit_arg(mut self, key: &str) -> Self {
        self.args.remove(key);
        self
    }

    pub fn omit_arg_mut(&mut self, key: &str) -> &mut Self {
        self.args.remove(key);
        self
    }

    #[must_use]
    pub fn omit_args(mut self, keys: impl IntoIterator<Item = String>) -> Self {
        for key in keys {
            self.args.remove(&key);
        }
        self
    }

    pub fn omit_args_mut(&mut self, keys: impl IntoIterator<Item = String>) -> &mut Self {
        for key in keys {
            self.args.remove(&key);
        }
        self
    }

    #[must_use]
    pub fn clear_args(mut self) -> Self {
        self.args.clear();
        self
    }

    pub fn clear_args_mut(&mut self) -> &mut Self {
        self.args.clear();
        self
    }
}

impl DatabaseRequest {
    pub async fn query<T>(self, db: &Database) -> Result<T, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        trace!(?self, "Querying database");
        db.query(&self.name, self.args).await
    }

    pub async fn mutate<T>(self, db: &Database) -> Result<T, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        trace!(?self, "Mutating database");
        db.mutation(&self.name, self.args).await
    }

    pub async fn watch_query<T>(
        self,
        db: &Database,
    ) -> Result<impl futures::Stream<Item = Result<T, ResponseError>>, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        trace!(?self, "Watching query");
        db.watch_query(&self.name, self.args).await
    }
}
