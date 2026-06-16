use app_helpers::encoding::{from_base64_padded, to_base64_padded};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha384;
use url::Url;

use crate::config::Config;

// Sha384 is used to properly fit into base64 (384 = 6 * 64 -> 64 base64 chars)
pub type SignatureHmac = Hmac<Sha384>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    #[serde(rename = "is", with = "signature_enc")]
    pub issued: DateTime<Utc>,
    #[serde(rename = "ex", with = "signature_enc")]
    pub expires: DateTime<Utc>,
    #[serde(rename = "hm")]
    pub hmac: String,
}
impl Signature {
    pub fn new<T>(for_data: T, expires: DateTime<Utc>) -> Self
    where
        T: AsRef<[u8]>,
    {
        let issued = Utc::now();

        let hmac = {
            let result = Self::generate_hmac(for_data, expires, issued);

            to_base64_padded(result.finalize().into_bytes())
        };

        Self {
            issued,
            expires,
            hmac,
        }
    }

    fn generate_hmac<T>(for_data: T, expires: DateTime<Utc>, issued: DateTime<Utc>) -> SignatureHmac
    where
        T: AsRef<[u8]>,
    {
        let mut mac = SignatureHmac::new_from_slice(Config::server().run.signing_key.as_bytes())
            .expect("Failed to create Hmac instance");
        mac.update(for_data.as_ref());
        mac.update(&signature_enc::timestamp_to_int(&issued).to_le_bytes());
        mac.update(&signature_enc::timestamp_to_int(&expires).to_le_bytes());

        mac
    }

    pub fn new_expires_in<T>(for_data: T, expires_in: Duration) -> Self
    where
        T: AsRef<[u8]>,
    {
        Self::new(for_data, Utc::now() + expires_in)
    }

    pub fn to_params(&self) -> Vec<(String, String)> {
        let vals = serde_json::to_value(self).expect("Failed to convert to value");
        let vals = vals.as_object().expect("Failed to convert to object");

        vals.iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect::<Vec<_>>()
    }

    pub fn to_absulute_url_from_path<TBase>(&self, base: TBase) -> Url
    where
        TBase: AsRef<str>,
    {
        let res = Config::server()
            .app
            .public_url
            .join(base.as_ref().trim_start_matches('/'))
            .expect("Failed to join url");

        self.add_params_to_url(res)
    }

    pub fn add_params_to_url(&self, mut url: Url) -> Url {
        url.query_pairs_mut().extend_pairs(self.to_params());

        url
    }

    pub fn validate<T>(&self, for_data: T) -> Result<(), SignatureError>
    where
        T: AsRef<[u8]>,
    {
        let self_hmac = match from_base64_padded(&self.hmac) {
            Ok(hmac) => hmac,
            Err(_) => return Err(SignatureError::MalformedHmac),
        };
        let valid_hmac = Self::generate_hmac(for_data, self.expires, self.issued);

        if valid_hmac.verify_slice(&self_hmac).is_err() {
            return Err(SignatureError::InvalidHmac);
        }

        if self.expires < Utc::now() {
            return Err(SignatureError::TimedOut);
        }

        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithDownloadUrl<T> {
    #[serde(flatten)]
    pub inner: T,
    pub download_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Failed to parse hmac")]
    MalformedHmac,
    #[error("Failed to verify hmac")]
    InvalidHmac,
    #[error("Signature expired")]
    TimedOut,
}

mod signature_enc {
    use app_helpers::encoding::BaseEncoding;
    use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S>(timestamp: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let unix_ts = timestamp_to_int(timestamp);
        let ts_as_str = unix_ts.to_base(36);
        serializer.serialize_str(&ts_as_str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ts_as_str = String::deserialize(deserializer)?;
        let unix_ts = i64::convert_from_base(&ts_as_str, 36).map_err(Error::custom)?;
        let ts_naive = int_to_timestamp(unix_ts)
            .ok_or_else(|| Error::custom(format!("Invalid timestamp: {}", unix_ts)))?;
        let ts = Utc.from_utc_datetime(&ts_naive);
        Ok(ts)
    }

    pub const fn timestamp_to_int(timestamp: &DateTime<Utc>) -> i64 {
        timestamp.timestamp()
    }

    pub const fn int_to_timestamp(int: i64) -> Option<NaiveDateTime> {
        let ts = match DateTime::from_timestamp(int, 0) {
            Some(ts) => ts,
            None => return None,
        };

        Some(ts.naive_utc())
    }
}
