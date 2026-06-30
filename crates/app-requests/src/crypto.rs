use rustls::crypto::{CryptoProvider, aws_lc_rs};

pub fn install_default_crypto_provider() -> Result<(), std::sync::Arc<CryptoProvider>> {
    aws_lc_rs::default_provider().install_default()
}
