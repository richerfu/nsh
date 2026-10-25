//! ArkTS-owned WebViews attached to native containers through checked bindings.
//! Controller work is ordered on a bounded worker queue; platform delegates
//! answer navigation policy synchronously from the mounted Rust snapshot.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use nsh_core::{Command, EventKind, EventPayload, PayloadValue, Size, WebProps, WebSource};
use nsh_engine::{Access, Assets, EngineControl};
use ohos_arkui_binding::{
    component::{attribute::ArkUICommonAttribute, built_in_component::Stack},
    types::advanced::LengthMetricUnit,
};
use openharmony_ability::NativeAttachment;
use openharmony_ability_plugin_webview::{
    NativeWebviewEvents, NativeWebviewSettings, WebviewCallbacksBuilder, WebviewClient,
    WebviewCreateRequest, WebviewDocumentOptionsRequest, WebviewHandle,
    WebviewInitializationScript, WebviewJavascriptProxyBuilder,
};

use crate::{
    app::{runtime::Dispatcher, session::Services},
    render::node::{NativeNode, Node, Result},
};

const BRIDGE: &str = "window.ReactNativeWebView={postMessage:function(data){window.__nshNative.postMessage(String(data));}};";
struct AssetReply(Option<openharmony_ability_plugin_webview::WebviewProtocolResponder>);
impl AssetReply {
    fn respond(mut self, response: ureq::http::Response<Vec<u8>>) {
        if let Some(responder) = self.0.take() {
            responder.respond(response);
        }
    }
}
impl Drop for AssetReply {
    fn drop(&mut self) {
        if let Some(responder) = self.0.take() {
            let mut response = ureq::http::Response::new(Vec::new());
            *response.status_mut() = ureq::http::StatusCode::SERVICE_UNAVAILABLE;
            responder.respond(response);
        }
    }
}
enum Operation {
    Configure(Box<WebProps>),
    Context(Option<u32>),
    Command(Command),
    Close,
}
pub(crate) struct WebView {
    owner: Rc<UiOwner>,
    close_route: Arc<Mutex<Option<Arc<Dispatcher>>>>,
    queue: mpsc::SyncSender<Operation>,
    active: Arc<AtomicBool>,
}
struct Worker {
    host: Arc<Services>,
    route: std::sync::Weak<Dispatcher>,
    control: EngineControl,
    node: u32,
    target: u32,
    id: String,
    access: Arc<Access>,
    assets: Arc<dyn Assets>,
    active: Arc<AtomicBool>,
    client: WebviewClient,
    handle: Option<WebviewHandle>,
    retiring_handle: Option<WebviewHandle>,
    props: Option<WebProps>,
    desired: Option<WebProps>,
    events: Option<NativeWebviewEvents>,
    ui_ready: bool,
    token: u32,
    close_route: Arc<Mutex<Option<Arc<Dispatcher>>>>,
}
// Failed retirement keeps the entire native owner on its UI thread.
thread_local! {
    static PENDING: RefCell<BTreeMap<u32, Rc<UiOwner>>> = RefCell::default();
}
struct UiOwner {
    token: u32,
    native: NativeNode,
    parent: Weak<RefCell<ohos_arkui_binding::common::node::ArkUINode>>,
    target: RefCell<Option<NativeAttachment>>,
    retired: RefCell<Vec<NativeAttachment>>,
    route: std::sync::Weak<Dispatcher>,
    closed: Cell<bool>,
}
impl UiOwner {
    fn detach_container(&self) -> Result<()> {
        let Some(parent) = self.parent.upgrade() else {
            self.native.borrow().remove_from_parent()?;
            return Ok(());
        };
        let index = parent
            .borrow()
            .children()
            .iter()
            .position(|child| Rc::ptr_eq(child, &self.native));
        if let Some(index) = index {
            if self.native.borrow().parent()?.is_some() {
                parent.borrow_mut().remove_child(index)?;
            } else {
                parent.borrow_mut().children_mut().remove(index);
            }
        } else {
            self.native.borrow().remove_from_parent()?;
        }
        Ok(())
    }
    fn retire_targets(&self) -> Result<()> {
        let mut retired = self.retired.borrow_mut();
        while let Some(target) = retired.last() {
            target.invalidate()?;
            retired.pop();
        }
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        self.retire_targets()?;
        if let Some(target) = self.target.borrow().as_ref() {
            target.invalidate()?;
        }
        self.target.borrow_mut().take();
        self.detach_container()?;
        if self.native.borrow().is_owned() {
            self.native.borrow_mut().dispose()?;
        }
        Ok(())
    }
}
pub(crate) fn retired(token: u32, created_target: Option<u32>, failure: Option<String>) {
    let owner = PENDING.with_borrow(|owners| owners.get(&token).cloned());
    let Some(owner) = owner else { return };
    let result = if let Some(target) = created_target {
        if owner.closed.get()
            || owner.target.borrow().as_ref().map(NativeAttachment::id) != Some(target)
        {
            return;
        }
        owner.retire_targets()
    } else if let Some(error) = failure {
        // The binding's disposal guard is the existing release boundary. Do
        // not invalidate a target when the SDK has not acknowledged retirement.
        let disposed = owner.native.borrow_mut().dispose();
        if let Err(disposal) = disposed {
            log::error!("WebView disposal failed; retaining owner: {error}; {disposal}");
            return;
        }
        owner.target.borrow_mut().take();
        owner.retired.borrow_mut().clear();
        Ok(())
    } else {
        owner.finish()
    };
    if let Err(error) = result {
        log::error!("WebView retirement failed; retaining owner: {error}");
    } else {
        PENDING.with_borrow_mut(|owners| owners.remove(&token));
    }
}
impl WebView {
    pub(crate) fn native(&self) -> &NativeNode {
        &self.owner.native
    }
    pub(crate) fn new(owner: Weak<Node>) -> Result<Self> {
        let owner = owner.upgrade().expect("live WebView owner");
        let client = owner.host.webview()?;
        let native = Stack::new()?.into_node();
        let target = (|| -> Result<NativeAttachment> {
            native
                .borrow()
                .set_length_metric_unit(LengthMetricUnit::Vp)?;
            Ok(NativeAttachment::new(&native)?)
        })();
        let target = match target {
            Ok(target) => target,
            Err(error) => {
                if let Err(cleanup) = native.borrow_mut().dispose() {
                    log::error!("WebView construction cleanup failed: {cleanup}");
                    std::mem::forget(native.clone());
                }
                return Err(error);
            }
        };
        let token = target.id();
        let ui = Rc::new(UiOwner {
            token,
            native,
            parent: Rc::downgrade(&owner.native),
            target: RefCell::new(Some(target)),
            retired: RefCell::default(),
            route: owner.dispatcher.clone(),
            closed: Cell::new(false),
        });
        let close_route = Arc::new(Mutex::new(None));
        let active = Arc::new(AtomicBool::new(true));
        let (queue, receiver) = mpsc::sync_channel(128);
        let worker = Worker {
            host: owner.host.clone(),
            route: owner.dispatcher.clone(),
            control: owner.control.clone(),
            node: owner.id,
            target: token,
            id: format!("nsh-native-{token}"),
            access: owner.access.clone(),
            assets: owner.assets.clone(),
            active: active.clone(),
            client,
            handle: None,
            retiring_handle: None,
            props: None,
            desired: None,
            events: None,
            ui_ready: owner.host.has_ui_context(),
            token,
            close_route: close_route.clone(),
        };
        let attached = { owner.native.borrow_mut().add_child(ui.native.clone()) };
        if let Err(error) = attached {
            if let Err(cleanup) = ui.finish() {
                log::error!("WebView construction cleanup failed; retaining owner: {cleanup}");
                PENDING.with_borrow_mut(|owners| owners.insert(token, ui));
            }
            return Err(error.into());
        }
        if let Err(error) = std::thread::Builder::new()
            .name("nsh-webview".into())
            .spawn(move || worker.run(receiver))
        {
            if let Err(cleanup) = ui.finish() {
                log::error!("WebView worker cleanup failed; retaining owner: {cleanup}");
                PENDING.with_borrow_mut(|owners| owners.insert(token, ui));
            }
            return Err(napi_ohos::Error::from_reason(error.to_string()).into());
        }
        Ok(Self {
            owner: ui,
            close_route,
            queue,
            active,
        })
    }
    pub(crate) fn configure(&self, props: Box<WebProps>) -> Result<()> {
        self.send(Operation::Configure(props))
    }
    pub(crate) fn context(&self, ready: bool) -> Result<()> {
        if self.owner.closed.get() {
            return Ok(());
        }
        let mut target = self.owner.target.borrow_mut();
        if !ready {
            if let Some(target) = target.take() {
                self.owner.retired.borrow_mut().push(target);
                PENDING
                    .with_borrow_mut(|owners| owners.insert(self.owner.token, self.owner.clone()));
            }
        } else if target.is_none() {
            *target = Some(NativeAttachment::new(self.native())?);
        }
        self.send(Operation::Context(
            target.as_ref().map(NativeAttachment::id),
        ))
    }
    pub(crate) fn command(&self, command: Command) -> Result<()> {
        self.send(Operation::Command(command))
    }
    fn send(&self, operation: Operation) -> Result<()> {
        if self.owner.closed.get() {
            return Ok(());
        }
        self.queue.try_send(operation).map_err(|_| {
            napi_ohos::Error::from_reason("WebView operation queue is full or closed")
        })?;
        Ok(())
    }
    pub(crate) fn frame(&self, size: Size) -> Result<()> {
        if self.owner.closed.get() {
            return Ok(());
        }
        self.native().borrow().width(size.width)?;
        self.native().borrow().height(size.height)?;
        Ok(())
    }
    pub(crate) fn close(&self) -> Result<()> {
        if !self.owner.closed.replace(true) {
            PENDING.with_borrow_mut(|owners| owners.remove(&self.owner.token));
            *self.close_route.lock().unwrap() = self.owner.route.upgrade();
            self.active.store(false, Ordering::Release);
            let detached = self.owner.detach_container();
            let _ = self.queue.try_send(Operation::Close);
            if let Err(error) = detached {
                PENDING
                    .with_borrow_mut(|owners| owners.insert(self.owner.token, self.owner.clone()));
                return Err(error);
            }
            // An earlier onClosing may already have released all native owners.
            let disposed = self.native().borrow_mut().dispose();
            if disposed.is_ok() {
                self.owner.target.borrow_mut().take();
                self.owner.retired.borrow_mut().clear();
            } else {
                PENDING
                    .with_borrow_mut(|owners| owners.insert(self.owner.token, self.owner.clone()));
            }
        }
        Ok(())
    }
}
impl Drop for WebView {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("OHOS WebView close: {e}");
        }
    }
}
impl Worker {
    fn live(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && self
                .route
                .upgrade()
                .is_some_and(|r| r.live.load(Ordering::Acquire))
    }
    fn run(mut self, receiver: mpsc::Receiver<Operation>) {
        for op in receiver {
            if matches!(op, Operation::Close) || !self.active.load(Ordering::Acquire) {
                break;
            }
            if !self.live() {
                continue;
            }
            let result = futures_executor::block_on(async {
                match op {
                    Operation::Configure(props) => {
                        self.desired = Some(props.as_ref().clone());
                        self.configure(*props).await
                    }
                    Operation::Context(target) => self.context(target).await,
                    Operation::Command(command) => self.command(command).await,
                    Operation::Close => Ok(()),
                }
            });
            if let Err(error) = result
                && self.live()
                && self.host.has_ui_context()
            {
                self.error(error.to_string());
            }
        }
        let disposed = self
            .handle
            .as_ref()
            .or(self.retiring_handle.as_ref())
            .map_or(Ok(()), |handle| {
                futures_executor::block_on(handle.dispose())
            });
        if let Some(route) = self.close_route.lock().unwrap().take() {
            route.send(crate::app::runtime::Message::WebviewRetired(
                self.token,
                None,
                disposed.err().map(|error| error.to_string()),
            ));
        }
        let _ = self.client.rollback_registrations(&self.id);
    }
    fn error(&self, message: String) {
        self.control.dispatch_event(
            self.node,
            EventKind::Error,
            EventPayload::Map(vec![("message", PayloadValue::Str(message))]),
        );
    }
    fn scripts(props: &WebProps) -> Vec<WebviewInitializationScript> {
        // Install the bridge before caller initialization as one atomic script.
        let script = props
            .injected_js_before
            .as_ref()
            .map_or_else(|| BRIDGE.to_owned(), |script| format!("{BRIDGE}\n{script}"));
        vec![WebviewInitializationScript {
            script,
            // ArkWeb requires a scheme-only rule for custom protocols.
            script_rules: vec!["*".into(), "nsh-asset://".into()],
        }]
    }
    fn settings(&self, props: &WebProps) -> NativeWebviewSettings {
        NativeWebviewSettings {
            id: self.id.clone(),
            scroll_enabled: props.scroll_enabled,
            injected_javascript: props.injected_js.as_deref().map(str::to_owned),
            html_base_url: match &props.source {
                WebSource::Html { base_url, .. } => base_url.as_deref().map(asset_url),
                _ => None,
            },
        }
    }
    async fn configure(&mut self, props: WebProps) -> napi_ohos::Result<()> {
        if self.props.as_ref() == Some(&props) && self.handle.is_some() {
            return Ok(());
        }
        if !self.ui_ready {
            self.props = Some(props);
            return Ok(());
        }
        if !self.access.allows_webviews()
            || matches!(&props.source, WebSource::Uri(uri) if !uri.starts_with("asset:/") && !self.access.allows_webview(uri))
        {
            return Err(napi_ohos::Error::from_reason(
                "WebView source is not granted",
            ));
        }
        let settings = self.settings(&props);
        let created = self.handle.is_none();
        if let Some(handle) = &self.handle {
            handle
                .update_document_options(WebviewDocumentOptionsRequest {
                    id: self.id.clone(),
                    javascript_enabled: Some(props.javascript_enabled),
                    initialization_scripts: Some(Self::scripts(&props)),
                })
                .await?;
            handle.configure_native(settings).await?;
            // Empty restores the SDK default after an explicit UA is removed.
            handle
                .set_user_agent(props.user_agent.as_deref().unwrap_or_default())
                .await?;
            if self
                .props
                .as_ref()
                .is_none_or(|old| old.source != props.source)
            {
                match &props.source {
                    WebSource::None => handle.load_url("about:blank").await?,
                    WebSource::Uri(uri) => handle.load_url(asset_url(uri)).await?,
                    WebSource::Html { html, .. } => handle.load_html(html.to_string()).await?,
                }
            }
        } else {
            if self.events.is_none() {
                self.declare()?;
            }
            let mut request = WebviewCreateRequest::new(self.id.clone());
            request.javascript_enabled = Some(props.javascript_enabled);
            request.user_agent = props.user_agent.as_deref().map(str::to_owned);
            request.initialization_scripts = Some(Self::scripts(&props));
            match &props.source {
                WebSource::None => request.url = Some("about:blank".into()),
                WebSource::Uri(uri) => request.url = Some(asset_url(uri)),
                WebSource::Html { html, .. } => request.html = Some(html.to_string()),
            }
            // A failed create may still own an entry whose retirement needs retry.
            self.retiring_handle = Some(self.client.handle(self.id.clone()));
            self.handle = Some(
                self.client
                    .create_native(self.target, request, settings)
                    .await?,
            );
        }
        self.props = Some(props);
        // A successful create awaited the old entry's full SDK removal.
        self.retiring_handle = None;
        if created && let Some(route) = self.route.upgrade() {
            route.send(crate::app::runtime::Message::WebviewRetired(
                self.token,
                Some(self.target),
                None,
            ));
        }
        Ok(())
    }
    async fn context(&mut self, target: Option<u32>) -> napi_ohos::Result<()> {
        self.ui_ready = target.is_some();
        if let Some(target) = target {
            self.target = target;
            if let Some(props) = self.desired.clone() {
                self.configure(props).await?;
            }
        } else {
            // Hook errors are isolated; preserve old ownership until a later
            // successful create confirms SDK cleanup of the previous entry.
            if let Some(handle) = self.handle.take() {
                self.retiring_handle = Some(handle);
            }
            self.events = None;
            self.props = None;
        }
        Ok(())
    }
    fn declare(&mut self) -> napi_ohos::Result<()> {
        let route = self.route.clone();
        let control = self.control.clone();
        let node = self.node;
        let host = Arc::downgrade(&self.host);
        let active = self.active.clone();
        let events = NativeWebviewEvents::new(
            self.host.app(),
            self.id.clone(),
            self.target,
            move |event| {
                if !active.load(Ordering::Acquire) {
                    return;
                }
                let Some(route) = route.upgrade().filter(|r| r.live.load(Ordering::Acquire)) else {
                    return;
                };
                let state = || {
                    EventPayload::Map(vec![
                        ("url", PayloadValue::Str(event.url.clone())),
                        ("title", PayloadValue::Str(event.title.clone())),
                        ("loading", PayloadValue::Bool(event.loading)),
                        ("canGoBack", PayloadValue::Bool(event.can_go_back)),
                        ("canGoForward", PayloadValue::Bool(event.can_go_forward)),
                    ])
                };
                match event.phase.as_str() {
                    "focus" | "blur" => {
                        let focus = event.phase == "focus";
                        let mut keyboard = route.keyboard.lock().unwrap();
                        if focus {
                            keyboard.focused = Some(node);
                        } else if keyboard.focused == Some(node) {
                            keyboard.focused = None;
                        }
                        drop(keyboard);
                        if focus && let Some(host) = host.upgrade() {
                            host.activate(Arc::downgrade(&route));
                        }
                        control.dispatch_event(
                            node,
                            if focus {
                                EventKind::Focus
                            } else {
                                EventKind::Blur
                            },
                            EventPayload::None,
                        );
                    }
                    // NativeWebviewEvents receives only main-frame errors from the Ability plugin.
                    "error" => {
                        control.dispatch_event(
                            node,
                            EventKind::Error,
                            EventPayload::Map(vec![
                                (
                                    "message",
                                    PayloadValue::Str(event.message.clone().unwrap_or_default()),
                                ),
                                (
                                    "code",
                                    PayloadValue::Number(event.code.unwrap_or_default() as f64),
                                ),
                                ("url", PayloadValue::Str(event.url.clone())),
                            ]),
                        );
                        control.dispatch_event(node, EventKind::LoadEnd, state());
                    }
                    "start" => {
                        control.dispatch_event(node, EventKind::LoadStart, state());
                        control.dispatch_event(node, EventKind::NavigationStateChange, state());
                    }
                    "finish" => {
                        control.dispatch_event(node, EventKind::Load, state());
                        control.dispatch_event(node, EventKind::LoadEnd, state());
                        control.dispatch_event(node, EventKind::NavigationStateChange, state());
                    }
                    _ => control.dispatch_event(node, EventKind::NavigationStateChange, state()),
                }
            },
        )?;
        let result = (|| {
            let access = self.access.clone();
            let route = self.route.clone();
            let active = self.active.clone();
            WebviewCallbacksBuilder::new(self.id.clone())
                .on_navigation_request(move |request| {
                    !active.load(Ordering::Acquire)
                        || route
                            .upgrade()
                            .is_none_or(|r| !r.live.load(Ordering::Acquire))
                        || !allows_navigation(&access, &request.url)
                })
                .build()?;
            let control = self.control.clone();
            let active = self.active.clone();
            WebviewJavascriptProxyBuilder::new(self.id.clone(), "__nshNative")
                .add_method("postMessage", move |_, args| {
                    if active.load(Ordering::Acquire)
                        && let Some(data) = args.first()
                    {
                        control.dispatch_event(
                            node,
                            EventKind::Message,
                            EventPayload::Map(vec![("data", PayloadValue::Str(data.clone()))]),
                        );
                    }
                })
                .build()?;
            self.asset_protocol()
        })();
        if let Err(error) = result {
            if let Err(cleanup) = self.client.rollback_registrations(&self.id) {
                log::error!("OHOS WebView declaration rollback: {cleanup}");
            }
            return Err(error);
        }
        // Publish readiness only after every declaration installed. Failure
        // drops the local event guard and the next configure retries all steps.
        self.events = Some(events);
        Ok(())
    }
    fn asset_protocol(&self) -> napi_ohos::Result<()> {
        let assets = self.assets.clone();
        let host = self.host.clone();
        self.client
            .custom_protocol_async(&self.id, "nsh-asset", move |_, request, _, responder| {
                let name = percent_encoding::percent_decode_str(request.uri().path())
                    .decode_utf8()
                    .map(|name| name.into_owned());
                let assets = assets.clone();
                let permit = host.reserve_load();
                let responder = AssetReply(Some(responder));
                let job = move || {
                    let result = permit.map_err(|e| e.to_string()).and_then(|_permit| {
                        name.as_ref()
                            .map_err(ToString::to_string)
                            .and_then(|name| assets.read(&format!("asset:{name}")))
                    });
                    let mime = match std::path::Path::new(name.as_deref().unwrap_or_default())
                        .extension()
                        .and_then(|x| x.to_str())
                    {
                        Some("html" | "htm") => "text/html",
                        Some("js" | "mjs") => "text/javascript",
                        Some("css") => "text/css",
                        Some("svg") => "image/svg+xml",
                        Some("png") => "image/png",
                        Some("jpg" | "jpeg") => "image/jpeg",
                        Some("woff2") => "font/woff2",
                        _ => "application/octet-stream",
                    };
                    let (status, bytes) = match result {
                        Ok(bytes) => (200, bytes),
                        Err(error) => (404, error.into_bytes()),
                    };
                    responder.respond(
                        ureq::http::Response::builder()
                            .status(status)
                            .header("content-type", mime)
                            .body(bytes)
                            .expect("valid scoped asset response"),
                    );
                };
                // Dropping the job on admission failure completes its request
                // with 503 through AssetReply, rather than leaving it pending.
                if let Err(error) =
                    crate::services::executor::Executor::assets().and_then(|pool| pool.submit(job))
                {
                    log::error!("OHOS WebView asset worker: {error}");
                }
            })
    }
    async fn command(&self, command: Command) -> napi_ohos::Result<()> {
        let Some(handle) = &self.handle else {
            return Err(napi_ohos::Error::from_reason("WebView has no document"));
        };
        match command {
            Command::Reload => handle.reload().await?,
            Command::GoBack => handle.go_back().await?,
            Command::GoForward => handle.go_forward().await?,
            Command::StopLoading => handle.stop_loading().await?,
            Command::Focus => handle.focus().await?,
            Command::Blur => {
                handle
                    .evaluate_script("document.activeElement?.blur();")
                    .await?;
            }
            Command::InjectJavaScript(script) => {
                handle.evaluate_script(script).await?;
            }
            Command::PostMessage(data) => {
                let literal = serde_json::to_string(&data).expect("string literal");
                handle
                    .evaluate_script(format!(
                        "window.dispatchEvent(new MessageEvent('message',{{data:{literal}}}));document.dispatchEvent(new MessageEvent('message',{{data:{literal}}}));"
                    ))
                    .await?;
            }
            _ => return Err(napi_ohos::Error::from_reason("invalid WebView command")),
        }
        Ok(())
    }
}
fn asset_url(uri: &str) -> String {
    uri.strip_prefix("asset:/")
        .map_or_else(|| uri.to_owned(), |path| format!("nsh-asset://app/{path}"))
}
fn allows_navigation(access: &Access, url: &str) -> bool {
    // ArkWeb represents loadData's already-authorized inline HTML as a data
    // URL in onLoadIntercept. It is not an additional network capability.
    (access.allows_webviews()
        && (url.is_empty()
            || url == "about:srcdoc"
            || url.starts_with("data:text/html;")
            || url.starts_with("nsh-asset://app/")))
        || access.allows_webview(url)
}
