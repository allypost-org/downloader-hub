use std::{net::IpAddr, sync::LazyLock};

use dns_lookup::lookup_host;
use ipnet::{Ipv4Net, Ipv6Net};
use iprange::IpRange;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum UrlIpValidationError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(Url),

    #[error("URL parse error: {0}")]
    UrlParse(url::ParseError),

    #[error("Invalid URL: no host")]
    NoHost,

    #[error("DNS lookup error: {0}")]
    DnsLookup(std::io::Error),

    #[error("Domain resolves to reserved IP: {0:?}")]
    ReservedIp(Vec<IpAddr>),
}

pub fn url_resolves_to_valid_ip(url: &str) -> Result<Url, UrlIpValidationError> {
    let parsed_url = Url::parse(url).map_err(UrlIpValidationError::UrlParse)?;

    if parsed_url.cannot_be_a_base() || !matches!(parsed_url.scheme(), "http" | "https") {
        return Err(UrlIpValidationError::InvalidUrl(parsed_url));
    }

    let url_host = match parsed_url.host() {
        Some(host) => host,
        None => {
            return Err(UrlIpValidationError::NoHost);
        }
    };

    let url_ips = match url_host {
        url::Host::Domain(domain) => lookup_host(domain)
            .map_err(UrlIpValidationError::DnsLookup)?
            .collect(),
        url::Host::Ipv4(ip) => {
            vec![ip.into()]
        }
        url::Host::Ipv6(ip) => {
            vec![ip.into()]
        }
    };

    let url_reserved_ips = url_ips
        .into_iter()
        .filter(|x| match &x {
            IpAddr::V4(ip) => RESERVED_RANGE_IPV4.contains(ip),
            IpAddr::V6(ip) => RESERVED_RANGE_IPV6.contains(ip),
        })
        .collect::<Vec<_>>();

    if !url_reserved_ips.is_empty() {
        return Err(UrlIpValidationError::ReservedIp(url_reserved_ips));
    }

    Ok(parsed_url)
}

pub static RESERVED_RANGE_IPV4: LazyLock<IpRange<Ipv4Net>> = LazyLock::new(|| {
    [
        "0.0.0.0/8",
        "10.0.0.0/8",
        "100.64.0.0/10",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "172.16.0.0/12",
        "192.0.0.0/24",
        "192.0.2.0/24",
        "192.88.99.0/24",
        "192.168.0.0/16",
        "198.18.0.0/15",
        "198.51.100.0/24",
        "203.0.113.0/24",
        "224.0.0.0/4",
        "233.252.0.0/24",
        "240.0.0.0/4",
        "255.255.255.255/32",
    ]
    .iter()
    .map(|s| s.trim().parse().expect("Failed to parse IP range"))
    .collect()
});

pub static RESERVED_RANGE_IPV6: LazyLock<IpRange<Ipv6Net>> = LazyLock::new(|| {
    [
        "::/128",
        "::1/128",
        "::ffff:0:0/96",
        "::ffff:0:0:0/96",
        "64:ff9b::/96",
        "64:ff9b:1::/48",
        "100::/64",
        "2001:0000::/32",
        "2001:20::/28",
        "2001:db8::/32",
        "2002::/16",
        "fc00::/7",
        "fe80::/10",
        "fe80::/64",
        "ff00::/8",
    ]
    .iter()
    .map(|s| s.trim().parse().expect("Failed to parse IP range"))
    .collect()
});
