//! NSH-owned SDK fragments that have no equivalent C attribute or constructor.
//! Rust owns policy and D2 targets retain SDK owners on the UI thread; workers receive data only.

use std::{cell::RefCell, path::PathBuf, rc::Weak, sync::Mutex};

use napi_derive_ohos::napi;
use napi_ohos::{Result, Unknown};
use openharmony_ability::{
    BridgeContextRequirement, BridgeMainThreadEvent, BridgePlugin, MainThreadSyncBridge,
    NativeAttachment, NativeNodeReady, NativeNodeRetiring, NativeSubtreeReady, OpenHarmonyApp,
    impl_bridge_napi_type,
};
use rustc_hash::FxHashMap;

use crate::render::node::{NativeNode, Node};

#[derive(Default)]
pub(crate) struct Plugin {
    information: Mutex<Option<HostInformation>>,
}
struct HostInformation {
    device_type: String,
    cache_directory: PathBuf,
}

impl BridgePlugin for Plugin {
    type Mode = MainThreadSyncBridge;
    const ID: &'static str = "nsh.arkui";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];

    fn required_contexts_for_main_thread_event(
        &self,
        event: &str,
    ) -> &'static [BridgeContextRequirement] {
        if event == "device-info" {
            &[BridgeContextRequirement::Ability]
        } else if event == "native-node-retiring" {
            &[]
        } else {
            &[BridgeContextRequirement::UiContext]
        }
    }

    fn on_main_thread_event<'env>(
        &self,
        event: BridgeMainThreadEvent<'env>,
    ) -> Result<Unknown<'env>> {
        let accepted = match event.name() {
            "device-info" => {
                let info = event.decode::<DeviceInfo>()?;
                let cache_directory = PathBuf::from(&info.cache_dir);
                if !cache_directory.is_absolute() || info.cache_dir.contains('\0') {
                    return Err(napi_ohos::Error::from_reason(
                        "Ability cache directory must be an absolute path",
                    ));
                }
                *self.information.lock().unwrap() = Some(HostInformation {
                    device_type: info.device_type.clone(),
                    cache_directory,
                });
                crate::app::session::device_type_changed(&info.device_type)?;
                true
            }
            "native-node-ready" => {
                event.decode::<NativeNodeReady>()?.attach(&event)?;
                true
            }
            "editor-event" => {
                let payload = event.decode::<EditorNotification>()?;
                let count = NativeAttachment::dispatch(payload.target_id, || {
                    let input = EDITORS.with(|routes| {
                        routes
                            .borrow()
                            .get(&payload.target_id)
                            .and_then(Weak::upgrade)
                    });
                    input
                        .map(|input| {
                            input
                                .native_event(payload.event)
                                .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))
                        })
                        .transpose()
                })??;
                return event.respond(EditorReceipt {
                    accepted: count.is_some(),
                    count: count.unwrap_or(0),
                });
            }
            "native-subtree-ready" => {
                let ready = event.decode::<NativeSubtreeReady>()?;
                let (_owner, inner) = VIEWS
                    .with(|views| {
                        views
                            .borrow()
                            .get(&ready.target_id)
                            .and_then(|route| route.owner.upgrade().zip(route.inner.upgrade()))
                    })
                    .ok_or_else(|| {
                        napi_ohos::Error::from_reason("Native subtree route is no longer live")
                    })?;
                // Prior SDK retirement may remove the receiver. Prepare the owned inner only
                // here, after those removals and immediately before D2 transfers it once.
                ohos_arkui_binding::common::node_registry::register(&inner.borrow(), inner.clone())
                    .and_then(|()| inner.borrow().add_event_receiver())
                    .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))?;
                ready.attach(&event)?;
                true
            }
            "native-node-retiring" => {
                event.decode::<NativeNodeRetiring>()?.detach()?;
                true
            }
            "view-action" => {
                let action = event.decode::<MenuAction>()?;
                NativeAttachment::dispatch(action.target_id, || {
                    let owner = VIEWS.with(|views| {
                        views
                            .borrow()
                            .get(&action.target_id)
                            .and_then(|route| route.owner.upgrade())
                    });
                    owner.is_some_and(|owner| {
                        let mut accepted = false;
                        crate::render::node::callback(|| {
                            if action.kind.is_some() {
                                accepted = owner.selection_action(&action)?;
                            } else {
                                let spec = owner.accessibility_state.spec.borrow();
                                if !spec.hidden && !spec.state.disabled && owner.visible_in_tree() {
                                    let registered = spec
                                        .actions
                                        .iter()
                                        .any(|item| item.name.as_ref() == action.action);
                                    drop(spec);
                                    if registered {
                                        owner.emit(
                                            nsh_core::EventKind::AccessibilityAction,
                                            nsh_core::EventPayload::Map(vec![(
                                                "actionName",
                                                nsh_core::PayloadValue::Str(action.action.clone()),
                                            )]),
                                        );
                                        accepted = true;
                                    } else if action.action == "activate"
                                        && owner
                                            .interaction
                                            .mask()
                                            .contains(nsh_core::EventKind::Press)
                                    {
                                        owner.emit(
                                            nsh_core::EventKind::Press,
                                            nsh_core::EventPayload::None,
                                        );
                                        accepted = true;
                                    }
                                }
                            }
                            Ok(())
                        });
                        accepted
                    })
                })?
            }
            _ => return Err(napi_ohos::Error::from_reason("Unknown NSH SDK event")),
        };
        event.respond(Acknowledgement { accepted })
    }
}

#[napi(object)]
pub(crate) struct DeviceInfo {
    pub device_type: String,
    pub cache_dir: String,
}
impl_bridge_napi_type!(DeviceInfo, "nsh.arkui.DeviceInfo");

#[napi(object)]
pub(crate) struct Acknowledgement {
    pub accepted: bool,
}
impl_bridge_napi_type!(Acknowledgement, "ohos.node.Acknowledgement");

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorSettings {
    pub target_id: u32,
    pub multiline: bool,
    pub secure: bool,
    pub text: String,
    pub count: u32,
    pub width: f64,
    pub height: f64,
    pub placeholder: String,
    pub font_size: f64,
    pub font_weight: u32,
    pub font_family: Option<String>,
    pub italic: bool,
    pub color: u32,
    pub placeholder_color: u32,
    pub caret_color: u32,
    pub selection_color: u32,
    pub editable: bool,
    pub hit_test: u32,
    pub align: u32,
    pub rtl: bool,
    pub capitalize: u32,
    pub keyboard: String,
    pub enter_key: String,
    pub blur_on_submit: bool,
    pub max_length: Option<u32>,
    pub auto_fill: bool,
    pub content_type: Option<String>,
    pub auto_focus: bool,
    pub accessibility: Option<EditorAccessibility>,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorAccessibility {
    pub label: String,
    pub hint: String,
    pub hidden: bool,
    pub accessible: Option<bool>,
    pub disabled: bool,
}

/// Focus and selection operations ordered after this batch's state updates.
#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorCommand {
    pub target_id: u32,
    pub kind: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Default)]
#[napi(object)]
pub(crate) struct EditorEvent {
    pub kind: String,
    pub text: String,
    pub count: u32,
    pub start: u32,
    pub end: u32,
    pub width: f64,
    pub height: f64,
}
#[napi(object)]
pub(crate) struct EditorNotification {
    pub target_id: u32,
    pub event: EditorEvent,
}
impl_bridge_napi_type!(EditorNotification, "nsh.arkui.EditorNotification");
#[napi(object)]
pub(crate) struct EditorReceipt {
    pub accepted: bool,
    pub count: u32,
}
impl_bridge_napi_type!(EditorReceipt, "nsh.arkui.EditorReceipt");
#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct MenuItem {
    pub name: String,
    pub label: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct Menu {
    pub generation: u32,
    pub visible: bool,
    pub x: f64,
    pub y: f64,
    pub items: Vec<MenuItem>,
}

#[napi(object)]
pub(crate) struct MenuAction {
    pub target_id: u32,
    pub action: String,
    pub kind: Option<String>,
    pub generation: Option<u32>,
}
impl_bridge_napi_type!(MenuAction, "nsh.arkui.MenuAction");

#[derive(Clone, Debug, Default, PartialEq)]
#[napi(object)]
pub(crate) struct TextureInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[napi(object)]
pub(crate) struct ViewOptions {
    pub target_id: u32,
    pub width: f64,
    pub height: f64,
    pub hue: f64,
    pub texture_surface: Option<String>,
    pub texture_insets: Option<TextureInsets>,
    pub test_id: Option<String>,
    pub semantic: bool,
    pub clickable: bool,
    pub disabled: bool,
    pub accessible: Option<bool>,
    pub hidden: bool,
    pub label: String,
    pub hint: String,
    pub value: String,
    pub role: String,
    pub checked: Option<bool>,
    pub selected: bool,
    pub api23: bool,
    pub api26: bool,
    pub actions: Vec<MenuItem>,
    pub menu: Option<Menu>,
}
impl_bridge_napi_type!(ViewOptions, "nsh.arkui.ViewOptions");

// Only weak routes live here. Nodes, native targets and engines retain their own lifetime.
struct ViewRegistration {
    owner: Weak<Node>,
    inner: std::rc::Weak<std::cell::RefCell<ohos_arkui_binding::common::node::ArkUINode>>,
}
thread_local! { static VIEWS: RefCell<FxHashMap<u32, ViewRegistration>> = RefCell::default(); }
pub(crate) struct ViewRoute {
    id: u32,
    owner: Weak<Node>,
}
impl ViewRoute {
    pub(crate) fn new(id: u32, owner: Weak<Node>, inner: &NativeNode) -> Self {
        VIEWS.with(|views| {
            views.borrow_mut().insert(
                id,
                ViewRegistration {
                    owner: owner.clone(),
                    inner: std::rc::Rc::downgrade(inner),
                },
            )
        });
        Self { id, owner }
    }
}
impl Drop for ViewRoute {
    fn drop(&mut self) {
        VIEWS.with(|views| {
            let mut views = views.borrow_mut();
            if views
                .get(&self.id)
                .is_some_and(|route| route.owner.ptr_eq(&self.owner))
            {
                views.remove(&self.id);
            }
        });
    }
}

pub(crate) fn device_type(app: &OpenHarmonyApp) -> Option<String> {
    let plugin = app.registered_plugin::<Plugin>().ok()??;
    plugin
        .information
        .lock()
        .unwrap()
        .as_ref()
        .map(|info| info.device_type.clone())
}

pub(crate) fn cache_directory(app: &OpenHarmonyApp) -> Result<PathBuf> {
    let plugin = app
        .registered_plugin::<Plugin>()?
        .ok_or_else(|| napi_ohos::Error::from_reason("NSH SDK plugin is unavailable"))?;
    plugin
        .information
        .lock()
        .unwrap()
        .as_ref()
        .map(|info| info.cache_directory.clone())
        .ok_or_else(|| napi_ohos::Error::from_reason("Ability cache directory is not ready"))
}

#[derive(Default, Clone)]
#[napi(object)]
pub(crate) struct Batch {
    pub editors: Vec<EditorSettings>,
    pub commands: Vec<EditorCommand>,
    pub remove_editors: Vec<u32>,
    pub views: Vec<ViewOptions>,
    pub remove_views: Vec<u32>,
}
impl_bridge_napi_type!(Batch, "nsh.arkui.Batch");
impl Batch {
    pub(crate) fn is_empty(&self) -> bool {
        self.editors.is_empty()
            && self.commands.is_empty()
            && self.remove_editors.is_empty()
            && self.views.is_empty()
            && self.remove_views.is_empty()
    }
}

pub(crate) fn apply(
    context: &crate::render::mount::UiContext,
    batch: Batch,
) -> crate::render::node::Result<()> {
    context
        .host
        .app()
        .with_main_thread_bridge(&context.env, |bridge| {
            let response: Acknowledgement = bridge.call_sync::<Plugin, _, _>("apply", batch)?;
            if response.accepted {
                Ok(())
            } else {
                Err(napi_ohos::Error::from_reason("NSH SDK batch rejected"))
            }
        })?;
    Ok(())
}

thread_local! { static EDITORS: RefCell<FxHashMap<u32, Weak<crate::input::editor::Input>>> = RefCell::default(); }
pub(crate) struct EditorRoute {
    id: u32,
}
impl EditorRoute {
    pub(crate) fn new(id: u32, input: Weak<crate::input::editor::Input>) -> Self {
        EDITORS.with(|routes| routes.borrow_mut().insert(id, input));
        Self { id }
    }
}
impl Drop for EditorRoute {
    fn drop(&mut self) {
        EDITORS.with(|routes| routes.borrow_mut().remove(&self.id));
    }
}
