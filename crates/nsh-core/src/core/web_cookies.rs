//! Read-only WebView cookie store queries in the core: validates the target
//! URL and `capabilities.webview`, sends [`Command::WebViewRequest`] with
//! [`WebViewOperation::GetCookies`], correlates one reply by request id and
//! node, and enforces deadlines and unmount cancellation.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{Core, Kind};
use crate::{
    Command, JsonValue, NodeId,
    element::{
        JsError, Task, codes,
        task::{self, Action, AnswerSlot},
    },
    webview::{
        WebViewOperation, WebViewRequest,
        cookies::{self, CookieQueryOptions, MAX_PENDING, WebCookie},
    },
};

pub(crate) type CookieSlot = AnswerSlot<Result<Vec<WebCookie>, JsError>>;

struct Pending {
    node: NodeId,
    name: Option<String>,
    answer: CookieSlot,
    _deadline: Task<()>,
}

#[derive(Default)]
pub(super) struct WebCookies {
    pending: FxHashMap<u64, Pending>,
}

impl WebCookies {
    pub(super) fn tracks(&self, request: u64) -> bool {
        self.pending.contains_key(&request)
    }
}

impl<M: crate::TextEngine> Core<M> {
    /// `ref.cookieStore.getAll(options)` / `ref.cookieStore.get(options)`:
    /// settles JS promise `call` with `CookieListItem[]` or `CookieListItem | null`.
    pub(super) fn start_js_web_cookies(
        &mut self,
        node: NodeId,
        call: u32,
        options: CookieQueryOptions,
        single: bool,
    ) {
        use crate::element::{Answer, IntoThrown, ToJs};
        let (answer, slot) = Answer::new();
        self.start_web_cookies(node, options, slot);
        let executor = Rc::downgrade(&self.components.executor);
        task::spawn_raw(
            &self.components.executor,
            crate::ROOT_ID,
            Box::pin(async move {
                let result = match answer.await {
                    Ok(cookies) => {
                        let json = if single {
                            serde_json::to_value(cookies.into_iter().next())
                        } else {
                            serde_json::to_value(cookies)
                        };
                        match json {
                            Ok(value) => Ok(Box::new(value) as Box<dyn ToJs>),
                            Err(err) => Err(IntoThrown::into_thrown(JsError::Error(format!(
                                "failed to encode web view cookies: {err}"
                            )))),
                        }
                    }
                    Err(err) => Err(IntoThrown::into_thrown(err)),
                };
                if let Some(executor) = executor.upgrade() {
                    executor.act(Action::Return(call, result));
                }
            }),
        );
    }

    pub(super) fn start_web_cookies(
        &mut self,
        node: NodeId,
        options: CookieQueryOptions,
        answer: CookieSlot,
    ) {
        if Rc::strong_count(&answer) == 1 {
            return;
        }
        let normalized_url = match cookies::validate_url(&options.url) {
            Ok(url) => url,
            Err(message) => {
                answer.borrow_mut().deliver(Err(JsError::Type(message)));
                return;
            }
        };
        if options.timeout.is_zero() {
            answer.borrow_mut().deliver(Err(JsError::Range(
                "cookie query timeout must be a positive duration".into(),
            )));
            return;
        }
        let env = &self.components.environment;
        let supported = env.web_views
            && (env.webview_requests
                || (env.os == crate::platform::Os::Android && env.web_evaluate));
        let node = self.native_id(node);
        let refused = if !supported {
            Some(
                JsError::Error("web views expose no cookie store here".into())
                    .with_code(codes::UNAVAILABLE),
            )
        } else if self
            .key(node)
            .ok()
            .is_none_or(|key| self.nodes[key as usize].kind != Kind::WebView)
        {
            Some(
                JsError::Error("the ref names no web view".into())
                    .with_code(codes::WEBVIEW_ABORTED),
            )
        } else if !self.access.allows_webviews() {
            Some(JsError::access_denied(
                "the app's capabilities do not grant webview",
                "webview",
                "",
            ))
        } else if !self.access.allows_webview(&normalized_url) {
            Some(JsError::access_denied(
                format!("web view cookie access to {normalized_url} is not granted"),
                "webview",
                &normalized_url,
            ))
        } else if self.components.web_cookies.pending.len() >= MAX_PENDING {
            Some(
                JsError::Error("WebView cookie query exceeds the pending request limit".into())
                    .with_code(codes::QUOTA_EXCEEDED),
            )
        } else {
            None
        };
        if let Some(error) = refused {
            answer.borrow_mut().deliver(Err(error));
            return;
        }
        let request = self.alloc_webview_request();
        let deadline = task::act_after(
            &self.components.executor,
            crate::ROOT_ID,
            options.timeout,
            Action::WebCookiesTimeout(request),
        );
        self.components.web_cookies.pending.insert(
            request,
            Pending {
                node,
                name: options.name,
                answer,
                _deadline: deadline,
            },
        );
        self.commands.push((
            node,
            Command::WebViewRequest(WebViewRequest {
                id: request,
                operation: WebViewOperation::GetCookies {
                    url: normalized_url,
                },
            }),
        ));
    }

    pub(super) fn answer_web_cookies(&mut self, node: NodeId, request: u64, reply: &JsonValue) {
        if !self
            .components
            .web_cookies
            .pending
            .get(&request)
            .is_some_and(|pending| pending.node == node)
        {
            return;
        }
        let pending = self
            .components
            .web_cookies
            .pending
            .remove(&request)
            .expect("checked request");
        let result = if reply.get("ok").and_then(JsonValue::as_bool) == Some(true) {
            let value = reply.get("value").unwrap_or(&JsonValue::Null);
            cookies::decode_reply_cookies(value, pending.name.as_deref())
                .map_err(|message| JsError::Error(message).with_code(codes::WEBVIEW_ABORTED))
        } else {
            let message = reply
                .get("error")
                .and_then(JsonValue::as_str)
                .unwrap_or("the web view failed to read cookies")
                .to_owned();
            Err(JsError::Error(message).with_code(codes::WEBVIEW_ABORTED))
        };
        pending.answer.borrow_mut().deliver(result);
    }

    pub(super) fn timeout_web_cookies(&mut self, request: u64) {
        if let Some(pending) = self.components.web_cookies.pending.remove(&request) {
            pending.answer.borrow_mut().deliver(Err(JsError::Error(
                "the web view cookie query did not answer in time".into(),
            )
            .with_code(codes::TIMED_OUT)));
        }
    }

    pub(super) fn cancel_web_cookies(&mut self, node: NodeId) {
        if self.components.web_cookies.pending.is_empty() {
            return;
        }
        self.components.web_cookies.pending.retain(|_, pending| {
            if pending.node != node {
                return true;
            }
            pending
                .answer
                .borrow_mut()
                .deliver(Err(JsError::Error("the web view went away".into())
                    .with_code(codes::WEBVIEW_ABORTED)));
            false
        });
    }
}
