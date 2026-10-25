//! NativeAbility session and UI-plugin ownership. Rendering belongs to explicit
//! NshSurface instances; the Ability root never creates an implicit app.

use std::{
    cell::RefCell,
    sync::{LazyLock, OnceLock},
};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, Result,
    bindgen_prelude::{Object, ObjectRef, Unknown},
};
use ohos_arkui_binding::ArkUIHandle;
use openharmony_ability::{
    AbilityInitContext, ApplicationLifecycle, BridgeMainThreadEvent, OpenHarmonyApp,
    PluginLifecycleEvent,
};

static APP: LazyLock<OpenHarmonyApp> = LazyLock::new(OpenHarmonyApp::new);
static CONFIGURED: OnceLock<()> = OnceLock::new();
thread_local! {
    static UI_OWNER: RefCell<Option<String>> = RefCell::default();
}

// These exports implement Ability's Module interface rather than another
// application plugin. An init failure releases the claimed bridge owner.
#[napi(ts_return_type = "object")]
pub(crate) fn init<'env>(
    env: &'env Env,
    bindings: ObjectRef,
    bridge_owner: String,
    #[napi(ts_arg_type = "object")] context: Option<Object<'env>>,
) -> Result<ApplicationLifecycle<'env>> {
    let context = AbilityInitContext::from_object(context.as_ref())?;
    if context.sdk_api_version.unwrap_or(0) < crate::app::features::MIN_API as i32 {
        return Err(Error::from_reason(format!(
            "Native Shell requires OHOS API {} or later",
            crate::app::features::MIN_API
        )));
    }
    openharmony_ability::version::init(
        context.sdk_api_version.unwrap_or(0),
        context.distribution_api_version.unwrap_or(0),
    );
    openharmony_ability::attach_bridge_session(env, bindings, &bridge_owner, &APP)?;
    APP.set_init_context(context);
    CONFIGURED.get_or_init(|| crate::app::session::configure(APP.clone()));
    match openharmony_ability::create_lifecycle_handle(env, APP.clone()) {
        Ok(lifecycle) => Ok(lifecycle),
        Err(error) => {
            APP.release_bridge_session(&bridge_owner);
            Err(error)
        }
    }
}

#[napi]
pub(crate) fn dispose_bridge(bridge_owner: String) {
    APP.release_bridge_session(&bridge_owner);
}

#[napi]
pub(crate) fn on_bridge_sync_event<'env>(
    env: &'env Env,
    plugin_id: String,
    event: String,
    request_type_name: String,
    response_type_name: String,
    value: Unknown<'env>,
) -> Result<Unknown<'env>> {
    APP.dispatch_bridge_main_thread_event(BridgeMainThreadEvent::new(
        env,
        plugin_id,
        event,
        request_type_name,
        response_type_name,
        value,
    )?)
}

#[napi]
pub(crate) fn on_bridge_lifecycle(kind: String) -> Result<()> {
    let event = PluginLifecycleEvent::from_arkts(&kind)?;
    APP.dispatch_plugin_lifecycle(event)?;
    if kind == "ui-context-ready" {
        crate::app::session::current()?.set_ui_ready(true)?;
    } else if let Ok(host) = crate::app::session::current() {
        // UI destruction may follow the Ability's session teardown.
        host.set_ui_ready(false)?;
    }
    Ok(())
}

// Required by Ability's Module interface. DefaultXComponent provides the UI
// context and plugin FrameNode tree; independent ContentSlots own NSH rendering.
#[napi]
pub(crate) fn render(
    #[napi(ts_arg_type = "object")] _slot: ArkUIHandle,
    render_owner: String,
    platform_window_id: Option<i32>,
) -> Result<()> {
    let host = crate::app::session::current()?;
    if render_owner.is_empty() {
        return Err(Error::from_reason("renderOwner must not be empty"));
    }
    UI_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_some() {
            return Err(Error::from_reason("Ability UI root is already attached"));
        }
        host.set_render_window(&render_owner, platform_window_id)?;
        *owner = Some(render_owner);
        Ok(())
    })
}
#[napi]
pub(crate) fn dispose_render(render_owner: String) {
    UI_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.as_ref() == Some(&render_owner) {
            *owner = None;
            if let Ok(host) = crate::app::session::current() {
                host.clear_render_window(&render_owner);
            }
        }
    });
}
#[napi]
pub(crate) fn dispose_all_renders() {
    UI_OWNER.with(|owner| {
        if let Some(owner) = owner.borrow_mut().take()
            && let Ok(host) = crate::app::session::current()
        {
            host.clear_render_window(&owner);
        }
    });
    crate::app::surface::dispose_all();
}

#[napi]
pub(crate) fn on_back_press_intercept() -> bool {
    crate::app::session::current().is_ok_and(|host| host.handle_back())
}
