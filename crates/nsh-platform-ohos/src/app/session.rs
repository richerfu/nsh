//! Ability owns plugins and their session. Each render attachment owns its
//! engine and reply queue; services retain only weak reply routes.

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use napi_ohos::{Error, Result};
use nsh_core::{EventKind, EventPayload, PayloadValue};
use openharmony_ability::{Event, OpenHarmonyApp};
use openharmony_ability_plugin_clipboard::{ClipboardBridgePlugin, ClipboardClient};
use openharmony_ability_plugin_resource::{ResourceBridgePlugin, ResourceExt};
use openharmony_ability_plugin_window::{WindowBridgePlugin, WindowExt};

use crate::app::{
    environment::NshEnvironment,
    runtime::{Dispatcher, Message},
};

const MAX_RESOURCE: usize = 64 << 20;
// The Ability event loop owns the session. This thread-local is only a weak
// lookup from N-API entry points; it never owns a surface or an engine.
thread_local! {
    static CURRENT: RefCell<Weak<Services>> = RefCell::default();
}

pub(crate) fn current() -> Result<Arc<Services>> {
    let services = CURRENT
        .with(|current| current.borrow().upgrade())
        .ok_or_else(|| {
            Error::from_reason("initialize NSH with NativeAbility before loading a surface")
        })?;
    services.check()?;
    Ok(services)
}

enum Operation {
    #[cfg(feature = "dev")]
    DevCopy {
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    },
    AccessibilityLive {
        route: Weak<Dispatcher>,
        text: String,
        assertive: bool,
    },
    StatusBar(Weak<Dispatcher>),
    AccessibilityEvent {
        route: Weak<Dispatcher>,
        request: openharmony_ability_plugin_accessibility::AccessibilitySendEventRequest,
    },
    Clipboard {
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    },
    Window,
    Accessibility,
    Stop,
}

pub(crate) struct Services {
    app: OpenHarmonyApp,
    active: AtomicBool,
    foreground: AtomicBool,
    ui_ready: AtomicBool,
    accessibility_subscribed: AtomicBool,
    motion: Mutex<Option<openharmony_ability_plugin_accessibility::AccessibilityMotionObservation>>,
    loads: AtomicUsize,
    resource_lock: Mutex<()>,
    environment: Mutex<NshEnvironment>,
    routes: Mutex<Vec<Weak<Dispatcher>>>,
    focused: Mutex<Weak<Dispatcher>>,
    selected: Mutex<Weak<crate::app::surface::Queue>>,
    overlays: Mutex<Vec<(u64, Weak<Dispatcher>)>>,
    next_overlay: AtomicU64,
    keyboard: Mutex<Option<crate::input::keyboard::KeyboardWindow>>,
    // Render identity is synchronous; installing a system key filter may await.
    window_identity: Mutex<Option<(String, i32)>>,
    queue: mpsc::SyncSender<Operation>,
}

pub(crate) struct Overlay {
    host: Weak<Services>,
    id: u64,
}
impl Drop for Overlay {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            host.overlays
                .lock()
                .unwrap()
                .retain(|(id, _)| *id != self.id);
        }
    }
}
pub(crate) struct LoadPermit(Arc<Services>);
impl Drop for LoadPermit {
    fn drop(&mut self) {
        self.0.loads.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Services {
    pub(crate) fn video_cache_directory(&self) -> Result<std::path::PathBuf> {
        self.check()?;
        crate::interop::sdk::cache_directory(&self.app)
    }
    pub(crate) fn set_render_window(&self, owner: &str, id: Option<i32>) -> Result<()> {
        let id = id.unwrap_or(0);
        if id < 0 {
            return Err(Error::from_reason("platformWindowId must be nonnegative"));
        }
        *self.window_identity.lock().unwrap() = Some((owner.to_owned(), id));
        if self.has_ui_context() {
            self.enqueue(Operation::Window)?;
        }
        Ok(())
    }
    pub(crate) fn clear_render_window(&self, owner: &str) {
        let removed = {
            let mut identity = self.window_identity.lock().unwrap();
            if identity
                .as_ref()
                .is_some_and(|(current, _)| current == owner)
            {
                identity.take();
                true
            } else {
                false
            }
        };
        if removed {
            self.keyboard.lock().unwrap().take();
        }
    }
    pub(crate) fn has_ui_context(&self) -> bool {
        self.ui_ready.load(Ordering::Acquire)
    }
    pub(crate) fn app(&self) -> &OpenHarmonyApp {
        &self.app
    }
    pub(crate) fn webview(&self) -> Result<openharmony_ability_plugin_webview::WebviewClient> {
        self.check()?;
        openharmony_ability_plugin_webview::WebviewClient::new(&self.app)
    }
    pub(crate) fn is_foreground(&self) -> bool {
        self.foreground.load(Ordering::Acquire)
    }
    fn set_foreground(&self, active: bool) {
        if !active && let Some(keyboard) = self.keyboard.lock().unwrap().as_ref() {
            keyboard.clear();
        }
        self.foreground.store(active, Ordering::Release);
        self.broadcast(|| Message::Active(active));
    }
    pub(crate) fn activate(&self, route: Weak<Dispatcher>) {
        if let Some(route) = route.upgrade() {
            *self.selected.lock().unwrap() = route.owner.clone();
        }
        *self.focused.lock().unwrap() = route;
        self.apply_selected_status();
    }
    pub(crate) fn select_surface(&self, owner: Weak<crate::app::surface::Queue>) {
        *self.selected.lock().unwrap() = owner;
        *self.focused.lock().unwrap() = Weak::new();
        self.apply_selected_status();
    }
    fn selected_route(&self) -> Option<Arc<Dispatcher>> {
        let selected = self.selected.lock().unwrap().clone();
        self.routes
            .lock()
            .unwrap()
            .iter()
            .rev()
            .filter_map(Weak::upgrade)
            .find(|r| r.live.load(Ordering::Acquire) && Weak::ptr_eq(&r.owner, &selected))
    }
    pub(crate) fn keyboard_route(&self) -> Option<Arc<Dispatcher>> {
        if !self.overlays.lock().unwrap().is_empty() {
            return None;
        }
        self.selected_route()
    }
    fn apply_selected_status(&self) {
        if let Some(route) = self.selected_route()
            && let Err(error) = self.enqueue(Operation::StatusBar(Arc::downgrade(&route)))
        {
            route.send(Message::Error(error.to_string()));
        }
    }
    pub(crate) fn status_bar(
        &self,
        route: Weak<Dispatcher>,
        config: nsh_core::StatusBarConfig,
        dark: bool,
    ) -> Result<()> {
        if let Some(route) = route.upgrade() {
            *route.status_bar.lock().unwrap() = Some((config, dark));
            self.enqueue(Operation::StatusBar(Arc::downgrade(&route)))?;
        }
        Ok(())
    }
    pub(crate) fn accessibility_event(
        &self,
        route: Weak<Dispatcher>,
        request: openharmony_ability_plugin_accessibility::AccessibilitySendEventRequest,
    ) -> Result<()> {
        self.enqueue(Operation::AccessibilityEvent { route, request })
    }
    pub(crate) fn accessibility_live(
        &self,
        route: Weak<Dispatcher>,
        text: String,
        assertive: bool,
    ) -> Result<()> {
        self.enqueue(Operation::AccessibilityLive {
            route,
            text,
            assertive,
        })
    }
    fn system_operation(
        &self,
        route: Weak<Dispatcher>,
        operation: impl std::future::Future<Output = Result<()>>,
    ) {
        if route
            .upgrade()
            .is_none_or(|r| !r.live.load(Ordering::Acquire))
        {
            return;
        }
        if let Err(error) = futures_executor::block_on(operation)
            && let Some(route) = route.upgrade()
        {
            route.send(Message::Error(error.to_string()));
        }
    }
    pub(crate) fn overlay(self: &Arc<Self>, route: Weak<Dispatcher>) -> Result<Overlay> {
        let id = self
            .next_overlay
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| Error::from_reason("overlay generation exhausted"))?;
        self.overlays.lock().unwrap().push((id, route));
        Ok(Overlay {
            host: Arc::downgrade(self),
            id,
        })
    }
    pub(crate) fn handle_back(&self) -> bool {
        if !self.is_foreground() {
            return false;
        }
        let overlay = {
            let mut overlays = self.overlays.lock().unwrap();
            overlays.retain(|(_, route)| {
                route
                    .upgrade()
                    .is_some_and(|r| r.live.load(Ordering::Acquire))
            });
            overlays.last().and_then(|(_, route)| route.upgrade())
        };
        let mut route = overlay.or_else(|| {
            self.focused
                .lock()
                .unwrap()
                .upgrade()
                .filter(|r| r.live.load(Ordering::Acquire))
        });
        if route.is_none() {
            let selected = self.selected.lock().unwrap().clone();
            let has_selection = selected.strong_count() > 0;
            route = self
                .routes
                .lock()
                .unwrap()
                .iter()
                .rev()
                .filter_map(Weak::upgrade)
                .find(|r| {
                    r.live.load(Ordering::Acquire)
                        && (!has_selection || Weak::ptr_eq(&r.owner, &selected))
                });
        }
        if let Some(route) = route
            && route.wants_back.load(Ordering::Acquire)
        {
            route.send(Message::Back);
            return true;
        }
        false
    }
    pub(crate) fn reserve_load(self: &Arc<Self>) -> Result<LoadPermit> {
        self.check()?;
        if self.loads.fetch_add(1, Ordering::AcqRel) >= 32 {
            self.loads.fetch_sub(1, Ordering::AcqRel);
            return Err(Error::from_reason(
                "at most 32 resource loads may be pending per Ability",
            ));
        }
        Ok(LoadPermit(self.clone()))
    }
    pub(crate) fn scale(&self) -> Result<f32> {
        let scale = self.app.scale();
        if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
            return Err(Error::from_reason(
                "host pixel ratio must be between 0.25 and 8",
            ));
        }
        Ok(scale)
    }
    pub(crate) fn set_ui_ready(&self, ready: bool) -> Result<()> {
        if !ready {
            self.keyboard.lock().unwrap().take();
        }
        self.ui_ready.store(ready, Ordering::Release);
        self.broadcast(|| Message::NativeContext(ready));
        if ready {
            self.enqueue(Operation::Window)?;
            self.enqueue(Operation::Accessibility)?;
        }
        Ok(())
    }

    fn new(app: OpenHarmonyApp, environment: NshEnvironment) -> Result<Arc<Self>> {
        let (queue, receiver) = mpsc::sync_channel(256);
        let services = Arc::new(Self {
            app,
            active: AtomicBool::new(true),
            foreground: AtomicBool::new(true),
            ui_ready: AtomicBool::new(false),
            accessibility_subscribed: AtomicBool::new(false),
            motion: Mutex::default(),
            loads: AtomicUsize::new(0),
            resource_lock: Mutex::new(()),
            environment: Mutex::new(environment),
            routes: Mutex::new(Vec::new()),
            focused: Mutex::new(Weak::new()),
            selected: Mutex::new(Weak::new()),
            overlays: Mutex::default(),
            next_overlay: AtomicU64::new(1),
            keyboard: Mutex::default(),
            window_identity: Mutex::default(),
            queue,
        });
        let weak = Arc::downgrade(&services);
        thread::Builder::new()
            .name("nsh-ohos-host".into())
            .spawn(move || {
                while let Ok(operation) = receiver.recv() {
                    if matches!(operation, Operation::Stop) {
                        break;
                    }
                    let Some(services) = weak.upgrade() else {
                        break;
                    };
                    if services.check().is_err() {
                        break;
                    }
                    match operation {
                        #[cfg(feature = "dev")]
                        Operation::DevCopy { route, revision, text } => services.copy_development(route, revision, text),
                        Operation::StatusBar(route) => {
                            if let Some(selected) = services.selected_route()
                                && Weak::ptr_eq(&route, &Arc::downgrade(&selected)) {
                                let settings = *selected.status_bar.lock().unwrap();
                                if let Some((config, dark)) = settings {
                                    services.system_operation(route, async {
                                        services.app.window()?.set_status_bar(openharmony_ability_plugin_window::WindowStatusBarRequest {
                                            window_id: 0, enabled: !config.hidden,
                                            light_content: match config.style { nsh_core::StatusBarStyle::Default => dark, nsh_core::StatusBarStyle::Light => true, nsh_core::StatusBarStyle::Dark => false },
                                            background: None, animated: config.animated,
                                        }).await
                                    });
                                }
                            }
                        }
                        Operation::AccessibilityLive { route, text, assertive } => services.system_operation(route, async {
                            openharmony_ability_plugin_accessibility::AccessibilityClient::new(&services.app)?.announce_live(text, assertive).await
                        }),
                        Operation::AccessibilityEvent { route, request } => services.system_operation(route, async {
                            openharmony_ability_plugin_accessibility::AccessibilityClient::new(&services.app)?.send_event(request).await
                        }),
                        Operation::Clipboard {
                            route,
                            text,
                            request,
                        } => services.clipboard(route, text, request),
                        Operation::Window => services.window(),
                        Operation::Accessibility => services.accessibility(),
                        Operation::Stop => unreachable!(),
                    }
                }
            })
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(services)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::from_reason("Ability plugin session is closed"))
        }
    }

    pub(crate) fn environment(&self) -> NshEnvironment {
        self.environment.lock().unwrap().clone()
    }

    pub(crate) fn attach(&self, route: Weak<Dispatcher>) -> Result<()> {
        self.check()?;
        let mut routes = self.routes.lock().unwrap();
        routes.retain(|route| route.strong_count() > 0);
        routes.push(route.clone());
        drop(routes);
        let selected = self.selected.lock().unwrap().clone();
        if selected.strong_count() == 0
            || route
                .upgrade()
                .is_some_and(|r| Weak::ptr_eq(&r.owner, &selected))
        {
            self.activate(route);
        }
        self.enqueue(Operation::Window)
    }

    fn broadcast(&self, message: impl Fn() -> Message) {
        let routes: Vec<_> = {
            let mut routes = self.routes.lock().unwrap();
            routes.retain(|route| route.strong_count() > 0);
            routes.iter().filter_map(Weak::upgrade).collect()
        };
        for route in routes {
            route.send(message());
        }
    }

    fn update(&self, changes: NshEnvironment) -> Result<()> {
        self.check()?;
        let mut environment = self.environment.lock().unwrap();
        let snapshot = environment
            .clone()
            .apply(nsh_engine::Environment::default())?;
        *environment = NshEnvironment::snapshot(changes.clone().apply(snapshot)?);
        drop(environment);
        self.broadcast(|| Message::HostEnvironment(changes.clone()));
        Ok(())
    }

    fn accessibility(self: &Arc<Self>) {
        let result = futures_executor::block_on(async {
            use openharmony_ability_plugin_accessibility::AccessibilityClient;
            let client = AccessibilityClient::new(&self.app)?;
            if crate::get_capabilities().api23 && self.motion.lock().unwrap().is_none() {
                let weak = Arc::downgrade(self);
                let observation = client
                    .observe_reduce_motion(move |enabled| {
                        if let Some(host) = weak.upgrade()
                            && host.check().is_ok()
                            && let Err(error) = host.update(NshEnvironment {
                                reduce_motion: Some(enabled),
                                ..Default::default()
                            })
                        {
                            log::warn!("OHOS reduced motion: {error}");
                        }
                    })
                    .await;
                match observation {
                    Ok(observation) => *self.motion.lock().unwrap() = Some(observation),
                    Err(error) => log::warn!("OHOS reduced-motion observation: {error}"),
                }
            }
            let font_scale = client.get_font_scale().await?;
            self.update(NshEnvironment {
                font_scale: Some(font_scale),
                ..Default::default()
            })?;
            let screen_reader = client.is_touch_explore_enabled().await?;
            self.update(NshEnvironment {
                font_scale: Some(font_scale),
                screen_reader: Some(screen_reader),
                ..Default::default()
            })?;
            if !self.accessibility_subscribed.load(Ordering::Acquire) {
                let weak = Arc::downgrade(self);
                client
                    .subscribe_state_change(move |enabled| {
                        if let Some(host) = weak.upgrade()
                            && host.check().is_ok()
                            && let Err(error) = host.update(NshEnvironment {
                                screen_reader: Some(enabled),
                                ..Default::default()
                            })
                        {
                            log::warn!("OHOS accessibility: {error}");
                        }
                    })
                    .await?;
                self.accessibility_subscribed.store(true, Ordering::Release);
            }
            Ok::<_, Error>(())
        });
        if let Err(error) = result {
            log::warn!("OHOS accessibility configuration: {error}");
        }
    }
    fn close(&self) {
        if self.active.swap(false, Ordering::AcqRel) {
            self.motion.lock().unwrap().take();
            self.broadcast(|| Message::HostClosed);
            let _ = self.queue.try_send(Operation::Stop);
        }
    }

    pub(crate) fn resource_reader(self: &Arc<Self>, name: &str) -> Result<Box<dyn std::io::Read>> {
        resource_name(name)?;
        self.check()?;
        let _lock = self.resource_lock.lock().unwrap();
        let manager = self
            .app
            .resource_manager()
            .ok_or_else(|| Error::from_reason("resource plugin is not ready"))?;
        let file = manager
            .open_raw_file(name)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Box::new(ResourceReader {
            file: Some(file),
            _manager: manager,
            host: self.clone(),
        }))
    }
    pub(crate) fn resource(&self, name: &str) -> Result<Vec<u8>> {
        resource_name(name)?;
        self.check()?;
        // One manager is shared by every engine and image worker in this host.
        let _lock = self.resource_lock.lock().unwrap();
        self.check()?;
        let manager = self
            .app
            .resource_manager()
            .ok_or_else(|| Error::from_reason("resource plugin is not ready"))?;
        let file = manager
            .open_raw_file(name)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        let bytes = file
            .read_bounded(MAX_RESOURCE)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        self.check()?;
        Ok(bytes)
    }

    #[cfg(feature = "dev")]
    pub(crate) fn copy_development_error(
        &self,
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    ) -> Result<()> {
        self.enqueue(Operation::DevCopy {
            route,
            revision,
            text,
        })
    }

    #[cfg(feature = "dev")]
    fn copy_development(
        &self,
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    ) {
        if route.strong_count() == 0 {
            return;
        }
        let result = futures_executor::block_on(async {
            ClipboardClient::new(&self.app)?.write_text(text).await
        })
        .map_err(|error| error.to_string());
        if self.check().is_ok()
            && let Some(route) = route.upgrade()
        {
            route.call(
                crate::app::surface::Message::DevCopied { revision, result },
                napi_ohos::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
            );
        }
    }

    pub(crate) fn enqueue_clipboard(
        &self,
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    ) -> Result<()> {
        self.enqueue(Operation::Clipboard {
            route,
            text,
            request,
        })
    }

    fn enqueue(&self, operation: Operation) -> Result<()> {
        self.check()?;
        self.queue
            .try_send(operation)
            .map_err(|_| Error::from_reason("Ability operation queue is full or closed"))
    }

    fn clipboard(
        &self,
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    ) {
        if route.strong_count() == 0 {
            return;
        }
        let result = futures_executor::block_on(async {
            let clipboard = ClipboardClient::new(&self.app)?;
            if let Some(text) = text {
                clipboard.write_text(text).await?;
            }
            let (has, text) = if request.is_some_and(|(_, has_only)| has_only) {
                let (reply, response) = mpsc::sync_channel(1);
                let dispatcher = route
                    .upgrade()
                    .ok_or_else(|| Error::from_reason("surface is closed"))?;
                dispatcher.send(Message::ClipboardProbe(reply));
                let has = response
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|_| {
                        Error::from_reason("clipboard presence query cancelled or timed out")
                    })?
                    .map_err(Error::from_reason)?;
                (has, String::new())
            } else if request.is_some() {
                let text = clipboard.read_text().await?.unwrap_or_default();
                (!text.is_empty(), text)
            } else {
                (false, String::new())
            };
            Ok::<_, Error>((has, text))
        });
        if self.check().is_err() {
            return;
        }
        if let Some(route) = route.upgrade() {
            if let Err(error) = &result {
                route.send(Message::Error(format!("clipboard: {error}")));
            }
            if let Some((id, _)) = request {
                let (has, text) = result.unwrap_or_default();
                route.send(Message::HostEvent(
                    EventKind::Clipboard,
                    EventPayload::Map(vec![
                        ("id", PayloadValue::Number(id as f64)),
                        ("has", PayloadValue::Bool(has)),
                        ("text", PayloadValue::Str(text)),
                    ]),
                ));
            }
        }
    }

    fn window(self: &Arc<Self>) {
        if !self.ui_ready.load(Ordering::Acquire) {
            return;
        }
        let identity = self.window_identity.lock().unwrap().clone();
        if identity.is_none() {
            return;
        }
        let result = futures_executor::block_on(async {
            let window = self.app.window()?;
            let id = i32::try_from(window.get_real_window_id(0).await?)
                .map_err(|_| Error::from_reason("window ID out of range"))?;
            if id <= 0
                || identity
                    .as_ref()
                    .is_some_and(|(_, expected)| *expected > 0 && *expected != id)
            {
                return Err(Error::from_reason(
                    "resolved window differs from the render owner",
                ));
            }
            self.check()?;
            if !self.has_ui_context() {
                return Err(Error::from_reason("UI context retired"));
            }
            {
                let mut current = self.window_identity.lock().unwrap();
                if *current != identity {
                    return Err(Error::from_reason("render owner changed"));
                }
                // Publish identity before awaiting ancillary window queries or
                // installing the key filter.
                current.as_mut().expect("active window owner").1 = id;
            }
            {
                let mut filter = self.keyboard.lock().unwrap();
                if filter.as_ref().is_none_or(|filter| filter.id() != id) {
                    filter.take();
                    match crate::input::keyboard::KeyboardWindow::new(id, Arc::downgrade(self)) {
                        Ok(window) => *filter = Some(window),
                        Err(error) => self.broadcast(|| Message::Error(error.to_string())),
                    }
                }
            }
            let system = window
                .query_avoid_area(openharmony_ability::AvoidAreaType::System)
                .await?;
            // Older hosts may not expose every visual type. Preserve the
            // supported system/keyboard observations when an optional query
            // is unavailable, rather than blocking window or IME readiness.
            let mut visual = [system, Default::default(), Default::default()];
            for (index, kind) in [
                openharmony_ability::AvoidAreaType::Cutout,
                openharmony_ability::AvoidAreaType::NavigationIndicator,
            ]
            .into_iter()
            .enumerate()
            {
                match window.query_avoid_area(kind).await {
                    Ok(area) => visual[index + 1] = area,
                    Err(error) => log::debug!("optional visual avoid area {kind:?}: {error}"),
                }
            }
            let keyboard = window
                .query_avoid_area(openharmony_ability::AvoidAreaType::Keyboard)
                .await?;
            Ok::<_, Error>((id, visual, keyboard))
        });
        if self.check().is_err() || !self.has_ui_context() {
            return;
        }
        match result {
            Ok((id, visual, keyboard)) => {
                let expected = identity.as_ref().map(|(owner, _)| (owner.clone(), id));
                if *self.window_identity.lock().unwrap() != expected {
                    return;
                }
                self.broadcast(|| Message::HostWindow(visual, keyboard));
            }
            Err(error) => {
                let current = self.window_identity.lock().unwrap().clone();
                if current.as_ref().map(|(owner, _)| owner)
                    == identity.as_ref().map(|(owner, _)| owner)
                {
                    self.broadcast(|| Message::Error(format!("window: {error}")));
                }
            }
        }
    }
}

fn idiom(device_type: &str) -> &'static str {
    match device_type {
        "tablet" | "pad" => "pad",
        "2in1" | "pc" | "desktop" => "desktop",
        "tv" => "tv",
        _ => "phone",
    }
}

pub(crate) fn device_type_changed(device_type: &str) -> Result<()> {
    if let Ok(host) = current() {
        host.update(NshEnvironment {
            idiom: Some(idiom(device_type).into()),
            ..Default::default()
        })?;
    }
    Ok(())
}

pub(crate) fn resource_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['\0', '\\'])
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Error::from_reason(
            "resource path must contain relative normal components",
        ));
    }
    Ok(())
}

struct HostRuntime {
    app: OpenHarmonyApp,
    session: Option<Arc<Services>>,
}

impl HostRuntime {
    fn observe(&mut self, event: Event<'_>) -> Result<()> {
        match event {
            Event::Create => {
                self.close();
                let environment = self.environment();
                environment
                    .clone()
                    .apply(nsh_engine::Environment::default())?;
                let session = Services::new(self.app.clone(), environment)?;
                CURRENT.with(|current| *current.borrow_mut() = Arc::downgrade(&session));
                self.session = Some(session);
            }
            Event::Destroy => self.close(),
            Event::Resume(_) | Event::Start => {
                if let Some(session) = &self.session {
                    session.set_foreground(true);
                    if session.ui_ready.load(Ordering::Acquire) {
                        session.enqueue(Operation::Accessibility)?;
                    }
                }
            }
            Event::Pause | Event::Stop | Event::WindowDestroy => {
                if let Some(session) = &self.session {
                    session.set_foreground(false);
                }
            }
            Event::ConfigChanged(_) => {
                if let Some(session) = &self.session {
                    session.update(self.environment())?;
                }
            }
            Event::AvoidAreaChange(_) | Event::SurfaceCreate => {
                if let Some(session) = &self.session {
                    session.enqueue(Operation::Window)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn environment(&self) -> NshEnvironment {
        let config = self.app.config();
        NshEnvironment {
            api_level: self
                .app
                .init_context()
                .sdk_api_version
                .and_then(|level| u32::try_from(level).ok()),
            font_scale: Some(config.font_size_scale),
            color_scheme: match config.color_mode {
                openharmony_ability::ColorMode::Dark => Some("dark".into()),
                openharmony_ability::ColorMode::Light => Some("light".into()),
                _ => None,
            },
            rtl: (!config.language.is_empty()).then(|| {
                matches!(
                    config.language.split(['-', '_']).next(),
                    Some("ar" | "he" | "fa" | "ur" | "ps" | "dv" | "yi")
                )
            }),
            idiom: crate::interop::sdk::device_type(&self.app)
                .map(|value| idiom(&value).to_owned()),
            locale: (!config.language.is_empty()).then_some(config.language),
            ..NshEnvironment::default()
        }
    }

    fn close(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = Weak::new());
        if let Some(session) = self.session.take() {
            session.close();
        }
    }
}

pub(crate) fn configure(app: OpenHarmonyApp) {
    app.register_plugin(crate::interop::sdk::Plugin::default())
        .expect("NSH SDK component contract");
    use openharmony_ability_plugin_webview::{WebviewProtocol, WebviewProtocolOptions};
    WebviewProtocol::register(
        "nsh-asset",
        WebviewProtocolOptions::Standard | WebviewProtocolOptions::Secure,
    )
    .expect("scoped WebView asset scheme");
    app.register_plugin(openharmony_ability_plugin_webview::WebviewBridgePlugin::new())
        .expect("webview plugin contract");
    app.set_frame_input_delivery_for(0, openharmony_ability::FrameInputDelivery::OnDemand)
        .expect("on-demand host frames");
    app.register_plugin(ResourceBridgePlugin::new())
        .expect("resource plugin contract");
    app.register_plugin(ClipboardBridgePlugin)
        .expect("clipboard plugin contract");
    app.register_plugin(WindowBridgePlugin)
        .expect("window plugin contract");
    app.register_plugin(
        openharmony_ability_plugin_accessibility::AccessibilityBridgePlugin::new(&app),
    )
    .expect("accessibility plugin contract");
    app.register_plugin(openharmony_ability_plugin_url::UrlBridgePlugin)
        .expect("URL plugin contract");
    let mut runtime = HostRuntime {
        app: app.clone(),
        session: None,
    };
    app.run_loop(move |event| {
        if let Err(error) = runtime.observe(event) {
            log::error!("OHOS host lifecycle: {error}");
        }
    });
}

// RawFile stays on the caller's IO worker. Every manager operation, including
// closing the file, shares the host's serialization lock.
struct ResourceReader {
    file: Option<openharmony_ability_plugin_resource::RawFile>,
    _manager: openharmony_ability_plugin_resource::ResourceManager,
    host: Arc<Services>,
}
impl std::io::Read for ResourceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.host.check().map_err(std::io::Error::other)?;
        let _lock = self.host.resource_lock.lock().unwrap();
        let file = self.file.as_ref().unwrap();
        let remaining = file.remain();
        if remaining < 0 {
            return Err(std::io::Error::other("invalid raw resource length"));
        }
        let count = buffer.len().min(remaining as usize).min(64 << 10);
        if count == 0 {
            return Ok(0);
        }
        // The baseline binding returns a capacity-sized Vec; verify its actual
        // consumption using the native file offset before publishing bytes.
        let before = file.offset();
        let bytes = file.read(count);
        let read = file
            .offset()
            .checked_sub(before)
            .filter(|n| *n > 0 && *n as usize <= count)
            .ok_or_else(|| std::io::Error::other("raw resource read failed"))?
            as usize;
        buffer[..read].copy_from_slice(&bytes[..read]);
        Ok(read)
    }
}
impl Drop for ResourceReader {
    fn drop(&mut self) {
        let _lock = self.host.resource_lock.lock().unwrap();
        self.file.take();
    }
}
