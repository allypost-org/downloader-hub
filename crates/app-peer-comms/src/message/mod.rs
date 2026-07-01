use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use iroh::{PublicKey, SecretKey, Signature, SignatureError};
use postcard::Error as PostcardError;
use serde::{Deserialize, Serialize};

use crate::jwt::JwtPair;

pub mod v1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Message {
    V1(v1::V1Message),
}

impl Message {
    #[must_use]
    pub fn v1<T>(data: T) -> Self
    where
        T: Into<v1::V1Message>,
    {
        Self::V1(data.into())
    }
}

impl Message {
    pub fn encode_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn decode_string(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn encode_bytes(&self) -> Result<Bytes, PostcardError> {
        postcard::to_stdvec(self).map(Into::into)
    }

    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, PostcardError> {
        postcard::from_bytes(bytes)
    }
}

impl Message {
    pub fn signed_and_encoded<T>(
        message: T,
        secret_key: &SecretKey,
        auth: Option<JwtPair>,
    ) -> Result<Bytes, SignedMessageError>
    where
        T: Into<Self>,
    {
        SignedMessage::sign_and_encode(secret_key, &message.into(), auth)
    }

    pub fn sign_and_encode(
        &self,
        secret_key: &SecretKey,
        auth: Option<JwtPair>,
    ) -> Result<Bytes, SignedMessageError> {
        SignedMessage::sign_and_encode(secret_key, self, auth)
    }

    pub fn verify_and_decode(
        bytes: &[u8],
    ) -> Result<(PublicKey, Self, Option<JwtPair>), SignedMessageError> {
        SignedMessage::verify_and_decode(bytes)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedMessage {
    from: PublicKey,
    data: Bytes,
    signature: Signature,
    auth: Option<JwtPair>,
    created_at: u64,
}

impl SignedMessage {
    #[must_use]
    pub fn with_auth(mut self, auth: JwtPair) -> Self {
        self.auth = Some(auth);
        self
    }
}

impl SignedMessage {
    pub fn verify_and_decode(
        bytes: &[u8],
    ) -> Result<(PublicKey, Message, Option<JwtPair>), SignedMessageError> {
        let signed_message: Self =
            postcard::from_bytes(bytes).map_err(SignedMessageError::DecodeSelf)?;

        let key: PublicKey = signed_message.from;
        key.verify(&signed_message.data, &signed_message.signature)
            .map_err(SignedMessageError::SignatureVerification)?;

        let message: Message =
            postcard::from_bytes(&signed_message.data).map_err(SignedMessageError::DecodeData)?;

        Ok((signed_message.from, message, signed_message.auth))
    }

    pub fn sign_and_encode(
        secret_key: &SecretKey,
        message: &Message,
        auth: Option<JwtPair>,
    ) -> Result<Bytes, SignedMessageError> {
        let data = message
            .encode_bytes()
            .map_err(SignedMessageError::EncodeData)?;

        let signature = secret_key.sign(&data);

        let encoded = postcard::to_stdvec(&Self {
            from: secret_key.public(),
            data,
            signature,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            auth,
        })
        .map_err(SignedMessageError::EncodeSelf)?;

        Ok(encoded.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignedMessageError {
    #[error("Failed to decode signed message: {0}")]
    DecodeSelf(PostcardError),

    #[error("Failed to verify signature: {0}")]
    SignatureVerification(SignatureError),

    #[error("Failed to decode data from message: {0}")]
    DecodeData(PostcardError),

    #[error("Failed to encode data from message: {0}")]
    EncodeData(PostcardError),

    #[error("Failed to encode signed message: {0}")]
    EncodeSelf(PostcardError),
}
