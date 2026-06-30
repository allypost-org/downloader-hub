use std::{sync::OnceLock, time::Duration};

use http::{HeaderMap, header};
pub use reqwest::{Client as RequestClient, ClientBuilder as RequestClientBuilder, RequestBuilder};
use rustls::{ClientConfig, RootCertStore, crypto::aws_lc_rs};
use webpki_root_certs::TLS_SERVER_ROOT_CERTS;

use super::url::UrlWithMeta;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub struct Client;

macro_rules! header_val {
    ($value:expr) => {
        $value.parse().expect("Failed to parse header value")
    };
}

impl Client {
    pub fn base() -> Result<RequestClient, String> {
        Self::builder()
            .build()
            .map_err(|e| format!("Failed to create client: {:?}", e))
    }

    pub fn sneaky() -> Result<RequestClient, String> {
        Self::builder()
            .default_headers(Self::sneaky_headers())
            .build()
            .map_err(|e| format!("Failed to create client: {:?}", e))
    }

    pub fn sneaky_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(
            header::USER_AGENT,
            header_val!("Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0"),
        );
        headers.insert(
            header::ACCEPT,
            header_val!("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
        );
        headers.insert(
            header::ACCEPT_LANGUAGE,
            header_val!("en-US,en;q=0.9,hr;q=0.8"),
        );
        headers.insert(header::ACCEPT_ENCODING, header_val!("gzip, deflate, br"));
        headers.insert(header::DNT, header_val!("1"));
        headers.insert("sec-gpc", header_val!("1"));
        headers.insert(header::CONNECTION, header_val!("keep-alive"));
        headers.insert(header::UPGRADE_INSECURE_REQUESTS, header_val!("1"));
        headers.insert("sec-fetch-dest", header_val!("document"));
        headers.insert("sec-fetch-mode", header_val!("navigate"));
        headers.insert("sec-fetch-site", header_val!("none"));
        headers.insert("sec-fetch-user", header_val!("?1"));
        headers.insert("priority", header_val!("u=4"));
        headers.insert(header::PRAGMA, header_val!("no-cache"));
        headers.insert(header::CACHE_CONTROL, header_val!("no-cache"));

        headers
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
