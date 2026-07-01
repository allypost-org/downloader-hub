use std::{process, thread, time};

use crate::encoding::to_base64;

fn now_ns() -> u128 {
    time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .map_or(0, |x| x.as_nanos())
}

#[must_use]
pub fn time_id() -> String {
    to_base64(now_ns().to_string())
}

#[must_use]
pub fn time_thread_id() -> String {
    let thread_id = thread::current().id();
    let process_id = process::id();
    let ns = now_ns();

    let id = format!("{ns}-{process_id}-{thread_id:?}");

    to_base64(id)
}
