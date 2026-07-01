use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub type ActionOptions = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub file_path: PathBuf,
    pub output_dir: PathBuf,
    pub action_options: ActionOptions,
}

impl ActionRequest {
    #[must_use]
    pub fn new(file_path: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            file_path,
            output_dir,
            action_options: HashMap::new(),
        }
    }

    pub fn in_same_dir(file_path: impl Into<PathBuf>) -> Option<Self> {
        let file_path = file_path.into();
        let output_dir = file_path.parent()?.to_path_buf();

        Some(Self::new(file_path, output_dir))
    }

    #[must_use]
    pub fn with_output_dir(mut self, output_dir: impl Into<PathBuf>) -> Self {
        self.output_dir = output_dir.into();
        self
    }

    #[must_use]
    pub fn options<T>(&self) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let val = serde_json::to_value(self.action_options.clone()).ok()?;

        serde_json::from_value(val).ok()
    }

    #[must_use]
    pub fn option<T>(&self, key: &str) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let val = self.action_options.get(key)?.clone();

        serde_json::from_value(val).ok()
    }

    #[must_use]
    pub fn with_option(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.action_options.insert(key.to_string(), value.into());
        self
    }

    #[must_use]
    pub fn with_options(mut self, options: ActionOptions) -> Self {
        self.action_options.extend(options);
        self
    }
}
