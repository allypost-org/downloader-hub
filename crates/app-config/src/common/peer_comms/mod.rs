pub use admin::*;
pub use bot::*;
pub use central::*;
pub use common::*;
pub use worker::*;

pub mod admin;
pub mod bot;
pub mod central;
pub mod common;
pub mod worker;

pub(crate) fn parse_slice_u8_32(s: &str) -> Result<[u8; 32], String> {
    let mut topic = [0u8; 32];
    let res = match data_encoding::HEXLOWER.decode(s.as_bytes()) {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    if res.len() != 32 {
        return Err("Topic must be 32 bytes long".to_string());
    }
    topic.copy_from_slice(&res);
    Ok(topic)
}
