use std::{sync::OnceLock, time::Duration};

pub use reqwest::{Client as RequestClient, ClientBuilder as RequestClientBuilder, RequestBuilder};
use rustls::{ClientConfig, RootCertStore, crypto::aws_lc_rs};
use webpki_root_certs::TLS_SERVER_ROOT_CERTS;

use super::url::UrlWithMeta;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub struct Client;

impl Client {
    pub fn base() -> Result<RequestClient, String> {
        Self::builder()
            .build()
            .map_err(|e| format!("Failed to create client: {:?}", e))
    }

    pub fn request_from_url(url: &UrlWithMeta) -> Result<RequestBuilder, String> {
        let mut builder = Self::base()?.request(url.method().clone(), url.url().as_str());

        for (k, v) in url.headers() {
            builder = builder.header(k, v);
        }

        Ok(builder)
    }

    pub fn builder() -> RequestClientBuilder {
        RequestClient::builder()
            .use_preconfigured_tls(tls_config())
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
    }
}

fn tls_config() -> ClientConfig {
    static CONFIG: OnceLock<ClientConfig> = OnceLock::new();

    CONFIG
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();

            for cert in TLS_SERVER_ROOT_CERTS.iter().cloned() {
                let _ = roots.add(cert);
            }

            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = roots.add(cert);
            }

            ClientConfig::builder_with_provider(aws_lc_rs::default_provider().into())
                .with_safe_default_protocol_versions()
                .expect("failed to enable rustls default protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth()
        })
        .clone()
}
