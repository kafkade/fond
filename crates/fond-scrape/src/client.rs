use std::time::Duration;

use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{self, HeaderMap, HeaderValue};

use crate::ScrapeError;

/// HTTP client for fetching recipe web pages.
///
/// Wraps `reqwest::blocking::Client` with sensible defaults for recipe
/// scraping: cookie jar, realistic user agent, timeouts, redirect limits.
pub struct ScrapeClient {
    client: Client,
}

/// Configuration for building a [`ScrapeClient`].
pub struct ScrapeClientConfig {
    /// User-Agent header value.
    pub user_agent: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum number of redirects to follow.
    pub max_redirects: usize,
}

impl Default for ScrapeClientConfig {
    fn default() -> Self {
        Self {
            user_agent: format!("fond/{} (recipe importer)", env!("CARGO_PKG_VERSION")),
            timeout_secs: 30,
            max_redirects: 5,
        }
    }
}

impl ScrapeClient {
    /// Create a new client with the default configuration.
    pub fn new() -> Result<Self, ScrapeError> {
        Self::with_config(ScrapeClientConfig::default())
    }

    /// Create a new client with custom configuration.
    pub fn with_config(config: ScrapeClientConfig) -> Result<Self, ScrapeError> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        default_headers.insert(
            header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9"),
        );

        let client = ClientBuilder::new()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout_secs))
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
            .cookie_store(true)
            .default_headers(default_headers)
            .build()
            .map_err(|e| ScrapeError::HttpError {
                url: "(client build)".into(),
                message: e.to_string(),
            })?;

        Ok(Self { client })
    }

    /// Fetch the HTML content of a URL.
    ///
    /// Returns the response body as a string. Fails on non-2xx status codes.
    pub fn fetch_html(&self, url: &str) -> Result<String, ScrapeError> {
        let response = self.client.get(url).send().map_err(|e| {
            if e.is_timeout() {
                ScrapeError::Timeout(url.to_string())
            } else {
                ScrapeError::HttpError {
                    url: url.to_string(),
                    message: e.to_string(),
                }
            }
        })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(ScrapeError::HttpStatus {
                url: url.to_string(),
                status,
            });
        }

        response
            .text()
            .map_err(|e| ScrapeError::InvalidEncoding(e.to_string()))
    }

    /// Access the underlying reqwest client for advanced use cases.
    pub fn inner(&self) -> &Client {
        &self.client
    }
}

impl Default for ScrapeClient {
    fn default() -> Self {
        Self::new().expect("failed to build default ScrapeClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = ScrapeClientConfig::default();
        assert!(config.user_agent.starts_with("fond/"));
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_redirects, 5);
    }

    #[test]
    fn client_builds_with_defaults() {
        let client = ScrapeClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn client_builds_with_custom_config() {
        let config = ScrapeClientConfig {
            user_agent: "test-agent/1.0".to_string(),
            timeout_secs: 10,
            max_redirects: 3,
        };
        let client = ScrapeClient::with_config(config);
        assert!(client.is_ok());
    }

    #[test]
    fn fetch_invalid_url_returns_error() {
        let client = ScrapeClient::new().unwrap();
        let result = client.fetch_html("not-a-url");
        assert!(result.is_err());
    }
}
