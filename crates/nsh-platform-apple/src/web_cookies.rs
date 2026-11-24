//! `WKWebView`'s `WKHTTPCookieStore` reader shared by macOS and iOS.
//!
//! Queries `WKWebView.configuration.websiteDataStore.httpCookieStore` via
//! `getAllCookies:`, which automatically honors `incognito` (`nonPersistentDataStore`)
//! and includes `HttpOnly` cookies, then filters against the target URL using
//! RFC 6265 rules in `nsh_core::webview::cookies::cookie_matches_url`.

use std::{
    cell::RefCell,
    time::{SystemTime, UNIX_EPOCH},
};

use nsh_core::{
    JsonValue,
    webview::{CookieSameSite, WebCookie, cookies},
};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool},
};
use objc2_foundation::{NSArray, NSDate, NSString};

struct AppleCookieFields {
    name: Option<Retained<NSString>>,
    value: Option<Retained<NSString>>,
    domain: Option<Retained<NSString>>,
    path: Option<Retained<NSString>>,
    expires_date: Option<Retained<NSDate>>,
    http_only: Bool,
    secure: Bool,
    same_site_raw: Option<Retained<NSString>>,
}

/// Reads cookies matching `url` from `web`'s `WKHTTPCookieStore` and passes
/// the serialized JSON array to `answer`.
pub fn get_cookies(
    web: &AnyObject,
    url: String,
    answer: impl FnOnce(Result<JsonValue, String>) + 'static,
) {
    // SAFETY: `web` is a live WKWebView on the main thread; `configuration`,
    // `websiteDataStore`, and `httpCookieStore` are documented WKWebView getters.
    let cookie_store: Option<Retained<AnyObject>> = unsafe {
        let config: Retained<AnyObject> = msg_send![web, configuration];
        let data_store: Retained<AnyObject> = msg_send![&config, websiteDataStore];
        msg_send![&data_store, httpCookieStore]
    };
    let Some(cookie_store) = cookie_store else {
        answer(Err("WKHTTPCookieStore is unavailable".into()));
        return;
    };
    let callback = RefCell::new(Some(
        Box::new(answer) as Box<dyn FnOnce(Result<JsonValue, String>)>
    ));
    let block = block2::RcBlock::new(move |raw_cookies: *mut NSArray<AnyObject>| {
        let Some(answer) = callback.borrow_mut().take() else {
            return;
        };
        if raw_cookies.is_null() {
            answer(Ok(JsonValue::Array(Vec::new())));
            return;
        }
        // SAFETY: WebKit invokes the completion handler on the main thread with
        // a valid `NSArray<NSHTTPCookie>` for the duration of this block.
        let array = unsafe { &*raw_cookies };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut matched = Vec::new();
        let count = array.count();
        for i in 0..count {
            if matched.len() >= 4096 {
                break;
            }
            let cookie = array.objectAtIndex(i);
            // SAFETY: `cookie` is an `NSHTTPCookie` instance inside the array.
            let fields = unsafe {
                AppleCookieFields {
                    name: msg_send![&cookie, name],
                    value: msg_send![&cookie, value],
                    domain: msg_send![&cookie, domain],
                    path: msg_send![&cookie, path],
                    expires_date: msg_send![&cookie, expiresDate],
                    http_only: msg_send![&cookie, isHTTPOnly],
                    secure: msg_send![&cookie, isSecure],
                    same_site_raw: msg_send![&cookie, sameSitePolicy],
                }
            };
            let domain_str = fields.domain.map(|s| s.to_string());
            let path_str = fields.path.map(|s| s.to_string());
            let expires_ms = fields
                .expires_date
                .and_then(|d| expires_to_ms(d.timeIntervalSince1970()));
            let is_secure = fields.secure.as_bool();
            if !cookies::cookie_matches_url(
                &url,
                domain_str.as_deref(),
                path_str.as_deref(),
                Some(is_secure),
                expires_ms,
                now_ms,
            ) {
                continue;
            }
            let mut record = WebCookie::new(
                fields.name.map(|s| s.to_string()).unwrap_or_default(),
                fields.value.map(|s| s.to_string()).unwrap_or_default(),
            );
            record.domain = domain_str;
            record.path = path_str;
            record.expires = expires_ms;
            record.http_only = Some(fields.http_only.as_bool());
            record.secure = Some(is_secure);
            record.same_site = fields
                .same_site_raw
                .as_deref()
                .and_then(|s| parse_same_site(&s.to_string()));
            matched.push(record);
        }
        let encoded = serde_json::to_value(matched)
            .map_err(|err| format!("failed to serialize cookies: {err}"));
        answer(encoded);
    });
    // SAFETY: `getAllCookies:` copies the block and runs it on the main thread.
    unsafe {
        let _: () = msg_send![&cookie_store, getAllCookies: &*block];
    }
}

fn expires_to_ms(seconds_since_1970: f64) -> Option<i64> {
    seconds_since_1970
        .is_finite()
        .then_some((seconds_since_1970 * 1000.0).round() as i64)
}

fn parse_same_site(raw: &str) -> Option<CookieSameSite> {
    match raw.to_ascii_lowercase().as_str() {
        "strict" => Some(CookieSameSite::Strict),
        "lax" => Some(CookieSameSite::Lax),
        "none" => Some(CookieSameSite::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_apple_cookie_metadata() {
        assert_eq!(parse_same_site("Strict"), Some(CookieSameSite::Strict));
        assert_eq!(parse_same_site("Lax"), Some(CookieSameSite::Lax));
        assert_eq!(parse_same_site("None"), Some(CookieSameSite::None));
        assert_eq!(parse_same_site(""), None);
        assert_eq!(expires_to_ms(1_700_000_000.5), Some(1_700_000_000_500));
        assert_eq!(expires_to_ms(f64::NAN), None);
    }
}
