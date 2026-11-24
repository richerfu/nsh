//! Read-only access to a web view's cookie store
//! (`WebViewRef.cookieStore` in TypeScript,
//! [`crate::element::ElementRef::cookies`] in Rust).
//!
//! Queries return [`WebCookie`] records matching a target HTTP(S) URL,
//! including `HttpOnly` cookies that scripts inside the page cannot read.
//! The core validates URLs, enforces `capabilities.webview`, applies
//! per-request timeouts, and routes requests and replies between the
//! engine and platform backends.

use std::{fmt, time::Duration};

use serde::{Deserialize, Serialize};

use crate::JsonValue;

/// Default timeout for cookie store queries (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum in-flight cookie store queries per engine.
pub(crate) const MAX_PENDING: usize = 64;

/// Maximum cookies accepted in one platform reply.
pub(crate) const MAX_COOKIES: usize = 4096;

/// `SameSite` policy of a cookie.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CookieSameSite {
    /// Sent only on first-party requests (`SameSite=Strict`).
    Strict,
    /// Sent on top-level cross-site navigations and first-party requests (`SameSite=Lax`).
    Lax,
    /// Sent on cross-site requests when `Secure` (`SameSite=None`).
    None,
}

/// A cookie read from a web view's store.
///
/// `name` and `value` are populated on every supported platform. Optional
/// metadata fields (`domain`, `path`, `expires`, `http_only`, `secure`,
/// `same_site`) are populated on macOS, iOS, Windows, and Linux and omitted
/// on Android because Android's public `CookieManager.getCookie(url)` API
/// exposes only `name=value` pairs.
#[non_exhaustive]
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebCookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value (redacted in `Debug` formatting).
    pub value: String,
    /// Domain attribute reported by the store, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Path attribute reported by the store, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Expiration timestamp in Unix epoch milliseconds (`None` for session
    /// cookies or when unavailable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
    /// Whether the cookie is marked `HttpOnly`, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// Whether the cookie is marked `Secure`, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    /// `SameSite` attribute, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<CookieSameSite>,
}

impl WebCookie {
    /// Creates a cookie record with `name` and `value` and no optional
    /// metadata fields set.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    /// Sets [`Self::domain`].
    #[must_use]
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Sets [`Self::path`].
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Sets [`Self::expires`] in Unix epoch milliseconds.
    #[must_use]
    pub fn with_expires(mut self, expires: i64) -> Self {
        self.expires = Some(expires);
        self
    }

    /// Sets [`Self::http_only`].
    #[must_use]
    pub fn with_http_only(mut self, http_only: bool) -> Self {
        self.http_only = Some(http_only);
        self
    }

    /// Sets [`Self::secure`].
    #[must_use]
    pub fn with_secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self
    }

    /// Sets [`Self::same_site`].
    #[must_use]
    pub fn with_same_site(mut self, same_site: CookieSameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }
}

impl fmt::Debug for WebCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebCookie")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("expires", &self.expires)
            .field("http_only", &self.http_only)
            .field("secure", &self.secure)
            .field("same_site", &self.same_site)
            .finish()
    }
}

/// Options for querying a web view's cookie store
/// ([`crate::element::ElementRef::cookies`]).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieQueryOptions {
    /// Target `http://` or `https://` URL to match cookies against.
    pub url: String,
    /// Optional exact cookie name filter.
    pub name: Option<String>,
    /// Maximum duration to wait for the platform cookie store
    /// ([`DEFAULT_TIMEOUT`] by default).
    pub timeout: Duration,
}

impl Default for CookieQueryOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            name: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl CookieQueryOptions {
    /// Creates cookie query options for `url` using [`DEFAULT_TIMEOUT`].
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    /// Filters results to cookies with the exact name `name`.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets [`Self::timeout`].
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Validates that `url` is a well-formed `http://` or `https://` URL with a
/// non-empty host and no ASCII control characters, returning the normalized
/// URL string.
#[doc(hidden)]
pub fn validate_url(url: &str) -> Result<String, String> {
    if url.is_empty() || url.bytes().any(|b| b.is_ascii_control()) {
        return Err("cookie query url must be a valid http(s) URL".into());
    }
    let parsed = url::Url::parse(url)
        .map_err(|err| format!("cookie query url must be a valid http(s) URL: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("cookie query url must use the http or https scheme".into());
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("cookie query url must include a host".into());
    }
    Ok(parsed.into())
}

/// Returns whether a cookie with the given attributes applies to `url` at
/// `now_ms` (Unix epoch milliseconds) according to RFC 6265 domain, path,
/// `Secure`, and expiration rules.
#[doc(hidden)]
pub fn cookie_matches_url(
    url: &str,
    domain: Option<&str>,
    path: Option<&str>,
    secure: Option<bool>,
    expires_ms: Option<i64>,
    now_ms: i64,
) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    if secure == Some(true) && parsed.scheme() != "https" {
        return false;
    }
    if expires_ms.is_some_and(|expires| expires <= now_ms) {
        return false;
    }
    let Some(req_host) = parsed.host_str() else {
        return false;
    };
    let req_host = req_host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let is_ip = matches!(parsed.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)));
    let Some(raw_domain) = domain.filter(|d| !d.is_empty()) else {
        return false;
    };
    let cookie_domain = raw_domain.to_ascii_lowercase();
    let domain_matches = if let Some(suffix) = cookie_domain.strip_prefix('.') {
        let suffix = suffix.trim_start_matches('[').trim_end_matches(']');
        if suffix.is_empty() {
            false
        } else if req_host == suffix {
            true
        } else {
            !is_ip
                && req_host.len() > suffix.len()
                && req_host.ends_with(suffix)
                && req_host.as_bytes()[req_host.len() - suffix.len() - 1] == b'.'
        }
    } else {
        let host_domain = cookie_domain.trim_start_matches('[').trim_end_matches(']');
        req_host == host_domain
    };
    if !domain_matches {
        return false;
    }
    let cookie_path = path.filter(|p| !p.is_empty()).unwrap_or("/");
    let req_path = parsed.path();
    if req_path == cookie_path {
        return true;
    }
    if let Some(rest) = req_path.strip_prefix(cookie_path) {
        return cookie_path.ends_with('/') || rest.starts_with('/');
    }
    false
}

/// Parses an HTTP `Cookie` header value (as returned by Android's
/// `CookieManager.getCookie`) into [`WebCookie`] records with `name` and
/// `value` populated and optional metadata fields omitted.
#[doc(hidden)]
pub fn parse_cookie_header(header: &str) -> Vec<WebCookie> {
    let mut cookies = Vec::new();
    if header.is_empty() {
        return cookies;
    }
    for part in header.split(';') {
        if cookies.len() >= MAX_COOKIES {
            break;
        }
        let part = part.trim();
        let Some((raw_name, raw_value)) = part.split_once('=') else {
            continue;
        };
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        cookies.push(WebCookie::new(name, raw_value.trim()));
    }
    cookies
}

/// Decodes a platform `WebViewReply` JSON value into [`WebCookie`] entries,
/// applying an optional exact `name` filter and capping at [`MAX_COOKIES`].
pub(crate) fn decode_reply_cookies(
    value: &JsonValue,
    name_filter: Option<&str>,
) -> Result<Vec<WebCookie>, String> {
    let cookies: Vec<WebCookie> = serde_json::from_value(value.clone())
        .map_err(|_| "the web view returned an invalid cookie reply".to_owned())?;
    let mut out = Vec::new();
    for cookie in cookies {
        if let Some(expected) = name_filter
            && cookie.name != expected
        {
            continue;
        }
        if out.len() >= MAX_COOKIES {
            break;
        }
        out.push(cookie);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_accepts_http_and_https_only() {
        assert_eq!(
            validate_url("https://Example.COM/path?a=1").unwrap(),
            "https://example.com/path?a=1"
        );
        assert_eq!(
            validate_url("http://127.0.0.1:8080/auth").unwrap(),
            "http://127.0.0.1:8080/auth"
        );
        for bad in [
            "",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "about:blank",
            "https://example.com/\nset-cookie",
            "https://",
            "https://:8080/path",
            "not a url",
        ] {
            assert!(validate_url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn cookie_matches_url_enforces_domain_path_secure_and_expiry() {
        let now = 1_700_000_000_000i64;
        // Domain cookie matches host and subdomains, never sibling suffix.
        assert!(cookie_matches_url(
            "https://app.example.com/v1/users",
            Some(".example.com"),
            Some("/v1"),
            Some(true),
            Some(now + 60_000),
            now,
        ));
        assert!(cookie_matches_url(
            "https://example.com/v1",
            Some(".example.com"),
            Some("/v1"),
            Some(false),
            None,
            now,
        ));
        assert!(!cookie_matches_url(
            "https://notexample.com/v1",
            Some(".example.com"),
            Some("/v1"),
            Some(false),
            None,
            now,
        ));

        // Host-only cookie never matches subdomains.
        assert!(!cookie_matches_url(
            "https://sub.example.com/",
            Some("example.com"),
            Some("/"),
            None,
            None,
            now,
        ));
        assert!(cookie_matches_url(
            "https://example.com/",
            Some("example.com"),
            Some("/"),
            None,
            None,
            now,
        ));

        // Path segment boundaries.
        assert!(!cookie_matches_url(
            "https://example.com/app2",
            Some("example.com"),
            Some("/app"),
            None,
            None,
            now,
        ));
        assert!(cookie_matches_url(
            "https://example.com/app/sub",
            Some("example.com"),
            Some("/app"),
            None,
            None,
            now,
        ));

        // Secure and expiration rules.
        assert!(!cookie_matches_url(
            "http://example.com/",
            Some("example.com"),
            Some("/"),
            Some(true),
            None,
            now,
        ));
        assert!(!cookie_matches_url(
            "https://example.com/",
            Some("example.com"),
            Some("/"),
            Some(false),
            Some(now),
            now,
        ));

        // IPv6 host match.
        assert!(cookie_matches_url(
            "http://[::1]:3000/api",
            Some("::1"),
            Some("/api"),
            None,
            None,
            now,
        ));
    }

    #[test]
    fn parse_cookie_header_and_debug_redaction() {
        let parsed = parse_cookie_header("session=abc=123; theme=dark; =ignored; empty=");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], WebCookie::new("session", "abc=123"));
        assert_eq!(parsed[1], WebCookie::new("theme", "dark"));
        assert_eq!(parsed[2], WebCookie::new("empty", ""));
        assert!(parse_cookie_header("").is_empty());

        let secret = WebCookie::new("sid", "super-secret-token")
            .with_http_only(true)
            .with_secure(true);
        let formatted = format!("{secret:?}");
        assert!(!formatted.contains("super-secret-token"), "{formatted}");
        assert!(formatted.contains("[REDACTED]"), "{formatted}");
    }
}
