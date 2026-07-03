use std::sync::Arc;

pub mod file;
pub mod request_info;

pub type RequestId = Arc<str>;
