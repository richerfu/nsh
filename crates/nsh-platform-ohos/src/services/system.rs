//! System UI requests are scoped to an engine generation. ArkTS owns their
//! controllers on the window thread; late completions hold only weak routes.
use std::sync::{
    Mutex, Weak,
    atomic::{AtomicU32, Ordering},
};

use nsh_core::{Command, EventKind, EventPayload, JsonValue, PayloadValue};
use rustc_hash::FxHashMap;

use crate::app::{
    runtime::{Dispatcher, Message},
    session::{HostCommand, Services},
};

#[derive(Clone)]
struct Request {
    route: Weak<Dispatcher>,
    kind: EventKind,
    id: u32,
    stream: bool,
    answer: bool,
}
#[derive(Default)]
pub(crate) struct System {
    next: AtomicU32,
    requests: Mutex<FxHashMap<u32, Request>>,
}
impl System {
    pub(crate) fn command(
        &self,
        host: &Services,
        route: Weak<Dispatcher>,
        command: Command,
    ) -> napi_ohos::Result<()> {
        use nsh_core::{notifications::NotificationCommand as N, speech::SpeechCommand as S};
        use serde_json::json;
        let (kind, id, stream, data) = match command {
            Command::Notification(command) => {
                let (id, stream, data) = match command {
                    N::Permission { id, ask } => {
                        (id, false, json!({"op":"notification-permission","ask":ask}))
                    }
                    N::Show { id, notification } => (
                        id,
                        false,
                        json!({"op":"notification-show","name":notification.id,"title":notification.title,"body":notification.body}),
                    ),
                    N::Close(name) => (0, false, json!({"op":"notification-close","name":name})),
                    N::SetBadge(count) => {
                        (0, false, json!({"op":"notification-badge","count":count}))
                    }
                    N::Listen => (0, true, json!({"op":"notification-listen"})),
                    _ => {
                        return Err(napi_ohos::Error::from_reason(
                            "unsupported notification command",
                        ));
                    }
                };
                (EventKind::Notification, id, stream, data)
            }
            Command::Speech(command) => {
                let (id, data) = match command {
                    S::Start { id, request } => (
                        id,
                        json!({"op":"speech-start","lang":request.lang,"interim":request.interim}),
                    ),
                    S::Stop { id } | S::Abort { id } => {
                        let key = self.requests.lock().unwrap().iter().find_map(|(&key, r)| {
                            (r.kind == EventKind::Speech
                                && r.id == id
                                && Weak::ptr_eq(&r.route, &route))
                            .then_some(key)
                        });
                        if let Some(key) = key {
                            let abort = matches!(command, S::Abort { .. });
                            if abort {
                                self.requests.lock().unwrap().remove(&key);
                            }
                            host.send_host_command(HostCommand {
                                kind: "system".into(),
                                open_id: Some(key),
                                text: Some(
                                    json!({"op":if abort {"speech-abort"} else {"speech-stop"}})
                                        .to_string(),
                                ),
                                ..Default::default()
                            })?;
                        }
                        return Ok(());
                    }
                    _ => return Err(napi_ohos::Error::from_reason("unsupported speech command")),
                };
                (EventKind::Speech, id, true, data)
            }
            Command::SetClipboard(text) => (
                EventKind::Clipboard,
                0,
                false,
                json!({"op":"clipboard-write","text":text}),
            ),
            Command::SetClipboardImage(uri) => (
                EventKind::Clipboard,
                0,
                false,
                json!({"op":"clipboard-write-image","uri":uri}),
            ),
            Command::ReadClipboard { id, has_only } => (
                EventKind::Clipboard,
                id,
                false,
                json!({"op":"clipboard-read","hasOnly":has_only}),
            ),
            Command::ReadClipboardImage { id, has_only } => (
                EventKind::Clipboard,
                id,
                false,
                json!({"op":"clipboard-image","hasOnly":has_only}),
            ),
            Command::ReadClipboardFiles { id } => (
                EventKind::Clipboard,
                id,
                false,
                json!({"op":"clipboard-files"}),
            ),
            _ => return Err(napi_ohos::Error::from_reason("unsupported system command")),
        };
        let answer = !matches!(
            data["op"].as_str(),
            Some(
                "clipboard-write"
                    | "clipboard-write-image"
                    | "notification-close"
                    | "notification-badge"
                    | "notification-listen"
            )
        );
        let key = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| napi_ohos::Error::from_reason("system request IDs exhausted"))?
            + 1;
        {
            let mut requests = self.requests.lock().unwrap();
            requests.retain(|_, r| {
                r.route
                    .upgrade()
                    .is_some_and(|d| d.live.load(Ordering::Acquire))
            });
            if requests.len() >= 256 {
                return Err(napi_ohos::Error::from_reason("too many system requests"));
            }
            requests.insert(
                key,
                Request {
                    route,
                    kind,
                    id,
                    stream,
                    answer,
                },
            );
        }
        if let Err(error) = host.send_host_command(HostCommand {
            kind: "system".into(),
            open_id: Some(key),
            text: Some(data.to_string()),
            ..Default::default()
        }) {
            self.requests.lock().unwrap().remove(&key);
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn complete(&self, key: u32, value: JsonValue) {
        let request = {
            let mut requests = self.requests.lock().unwrap();
            let Some(request) = requests.get(&key).cloned() else {
                return;
            };
            if !request.stream || value["phase"] == "end" {
                requests.remove(&key);
            }
            request
        };
        let Some(route) = request
            .route
            .upgrade()
            .filter(|d| d.live.load(Ordering::Acquire))
        else {
            return;
        };
        let payload = if request.kind == EventKind::Speech {
            use nsh_core::speech::{SpeechError, SpeechEvent as E};
            let event = match value["phase"].as_str() {
                Some("start") => E::Start,
                Some("result") => E::Result {
                    text: value["text"].as_str().unwrap_or_default().into(),
                    is_final: value["final"].as_bool().unwrap_or(false),
                },
                Some("end") => E::End {
                    error: value["error"].as_str().and_then(SpeechError::from_name),
                },
                _ => return,
            };
            EventPayload::Speech {
                id: request.id,
                event,
            }
        } else {
            if let Some(error) = value["error"].as_str() {
                route.send(Message::Error(format!("system service: {error}")));
            }
            let mut fields = Vec::new();
            if request.answer {
                fields.push(("id", PayloadValue::Number(f64::from(request.id))));
            }
            for key in [
                "permission",
                "shown",
                "pressed",
                "has",
                "text",
                "uri",
                "width",
                "height",
                "tooLarge",
                "files",
            ] {
                let field = match &value[key] {
                    JsonValue::String(v) => PayloadValue::Str(v.clone()),
                    JsonValue::Bool(v) => PayloadValue::Bool(*v),
                    JsonValue::Number(v) => PayloadValue::Number(v.as_f64().unwrap_or_default()),
                    _ => continue,
                };
                fields.push((key, field));
            }
            if fields.is_empty() {
                return;
            }
            EventPayload::Map(fields)
        };
        route.send(Message::HostEvent(request.kind, payload));
    }
    pub(crate) fn retire(&self, host: &Services, route: &Weak<Dispatcher>) {
        let retired: Vec<u32> = {
            let mut requests = self.requests.lock().unwrap();
            let keys = requests
                .iter()
                .filter_map(|(&key, r)| Weak::ptr_eq(&r.route, route).then_some(key))
                .collect::<Vec<_>>();
            for key in &keys {
                requests.remove(key);
            }
            keys
        };
        for key in retired {
            let _ = host.send_host_command(HostCommand {
                kind: "system-cancel".into(),
                open_id: Some(key),
                ..Default::default()
            });
        }
    }
}
