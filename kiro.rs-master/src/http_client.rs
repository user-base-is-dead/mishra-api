//! HTTP Client build module
//!
//! provideunifyof HTTP Client Build feature, supports proxy config.

use reqwest::{Client, Proxy};
use std::time::Duration;

use crate::model::config::TlsBackend;

/// proxy config
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ProxyConfig {
    /// proxy address, supports http/https/socks5
    pub url: String,
    /// proxy authentication username
    pub username: Option<String>,
    /// proxyauthpassword
    pub password: Option<String>,
}

impl ProxyConfig {
    /// from url createproxy config
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            username: None,
            password: None,
        }
    }

    /// setauthinfo
    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

/// build HTTP Client
///
/// # Arguments
/// * `proxy` - optional proxy configuration
/// * `timeout_secs` - timeout duration (seconds)
///
/// # Returns
/// configured reqwest::Client
pub fn build_client(
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
    tls_backend: TlsBackend,
) -> anyhow::Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(timeout_secs));

    match tls_backend {
        TlsBackend::Rustls => {
            builder = builder.use_rustls_tls();
        }
        TlsBackend::NativeTls => {
            #[cfg(feature = "native-tls")]
            {
                builder = builder.use_native_tls();
            }
            #[cfg(not(feature = "native-tls"))]
            {
                anyhow::bail!("this build version does not include native-tls backend, please switch in the configuration to use rustls");
            }
        }
    }

    if let Some(proxy_config) = proxy {
        let mut proxy = Proxy::all(&proxy_config.url)?;

        // setproxyauth
        if let (Some(username), Some(password)) = (&proxy_config.username, &proxy_config.password) {
            proxy = proxy.basic_auth(username, password);
        }

        builder = builder.proxy(proxy);
        tracing::debug!("HTTP Client use proxy: {}", proxy_config.url);
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_new() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        assert_eq!(config.url, "http://127.0.0.1:7890");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_proxy_config_with_auth() {
        let config = ProxyConfig::new("socks5://127.0.0.1:1080").with_auth("user", "pass");
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_build_client_without_proxy() {
        let client = build_client(None, 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_with_proxy() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        let client = build_client(Some(&config), 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }
}
