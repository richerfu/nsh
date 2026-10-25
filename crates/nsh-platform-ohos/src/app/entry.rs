//! OHOS host session and UI context ownership. Host apps use standard `UIAbility`;
//! the runtime session and `UIContext` attachment initialize automatically on demand.

use std::cell::RefCell;

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, Result,
    bindgen_prelude::{Function, FunctionRef, Unknown},
};
use ohos_resource_manager_binding::ResourceManager;

use crate::app::{
    environment::NshEnvironment,
    runtime::Message,
    session::{AvoidArea, HostCommand, HostInit, Services},
};

thread_local! {
    static UI_OWNER: RefCell<Option<String>> = RefCell::default();
    static BOOTSTRAP: RefCell<Option<FunctionRef<(), ()>>> = RefCell::default();
}

#[napi]
pub(crate) fn nsh_register_bootstrap(
    sdk_api_version: i32,
    #[napi(ts_arg_type = "() => void")] bootstrap: Function<'_, (), ()>,
) -> Result<()> {
    crate::app::features::set_runtime_api_version(sdk_api_version);
    crate::media::webview::register_custom_scheme();
    let reference = bootstrap.create_ref()?;
    BOOTSTRAP.with(|slot| {
        *slot.borrow_mut() = Some(reference);
    });
    Ok(())
}

pub(crate) fn ensure_host_ready(env: &Env) -> Result<()> {
    let ready =
        crate::app::session::current().is_ok() && UI_OWNER.with(|owner| owner.borrow().is_some());
    if ready {
        return Ok(());
    }
    let has_bootstrap = BOOTSTRAP.with(|slot| slot.borrow().is_some());
    if !has_bootstrap {
        let _ = env.load("@nsh/ohos");
    }
    let bootstrap = BOOTSTRAP.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|reference| reference.borrow_back(env))
            .transpose()
    })?;
    if let Some(bootstrap) = bootstrap {
        bootstrap.call(())?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_init_host(
    init: HostInit,
    #[napi(ts_arg_type = "object")] resource_manager: Unknown<'_>,
    #[napi(ts_arg_type = "(command: HostCommand) => void")] on_command: Function<
        '_,
        HostCommand,
        (),
    >,
) -> Result<()> {
    if init.sdk_api_version < crate::app::features::MIN_API as i32 {
        return Err(Error::from_reason(format!(
            "Native Shell requires OHOS API {} or later",
            crate::app::features::MIN_API
        )));
    }
    crate::app::features::set_runtime_api_version(init.sdk_api_version);
    crate::media::webview::register_custom_scheme();
    let resource_manager = ResourceManager::from_napi_value(resource_manager)?;
    let command_tsfn = on_command
        .build_threadsafe_function::<HostCommand>()
        .callee_handled::<false>()
        .build()?;
    let session = Services::new(init, resource_manager, command_tsfn)?;
    crate::app::session::replace_session(Some(session));
    Ok(())
}

#[napi]
pub(crate) fn nsh_destroy_host(env: Env) {
    dispose_all_renders(env);
    crate::app::session::replace_session(None);
}

#[napi]
pub(crate) fn nsh_set_foreground(foreground: bool) {
    if let Ok(host) = crate::app::session::current() {
        host.set_foreground(foreground);
    }
}

#[napi]
pub(crate) fn nsh_update_config(
    color_mode: i32,
    font_size_scale: f64,
    language: String,
    device_type: Option<String>,
) -> Result<()> {
    if let Ok(host) = crate::app::session::current() {
        host.update(crate::app::session::environment_from_config(
            color_mode,
            font_size_scale,
            language,
            device_type.as_deref(),
        ))?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_update_accessibility(
    screen_reader: Option<bool>,
    reduce_motion: Option<bool>,
) -> Result<()> {
    if let Ok(host) = crate::app::session::current() {
        host.update(NshEnvironment {
            screen_reader,
            reduce_motion: reduce_motion.filter(|_| crate::get_capabilities().api23),
            ..Default::default()
        })?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_attach_ui(
    render_owner: String,
    platform_window_id: Option<i32>,
    #[napi(ts_arg_type = "(batch: Batch) => void")] apply_batch: Function<
        '_,
        crate::interop::sdk::Batch,
        (),
    >,
) -> Result<()> {
    let host = crate::app::session::current()?;
    if render_owner.is_empty() {
        return Err(Error::from_reason("renderOwner must not be empty"));
    }
    let applier = apply_batch.create_ref()?;
    UI_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_some() {
            return Err(Error::from_reason("Ability UI root is already attached"));
        }
        host.set_render_window(&render_owner, platform_window_id)?;
        crate::app::session::set_ui_batch_applier(Some(applier));
        if let Err(error) = host.set_ui_ready(true) {
            crate::app::session::set_ui_batch_applier(None);
            host.clear_render_window(&render_owner);
            return Err(error);
        }
        *owner = Some(render_owner);
        Ok(())
    })
}

#[napi]
pub(crate) fn nsh_detach_ui(_env: Env, render_owner: String) {
    UI_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.as_ref() == Some(&render_owner) {
            *owner = None;
            crate::app::session::set_ui_batch_applier(None);
            if let Ok(host) = crate::app::session::current() {
                let _ = host.set_ui_ready(false);
                host.clear_render_window(&render_owner);
            }
        }
    });
}

#[napi]
pub(crate) fn nsh_update_window(
    render_owner: String,
    platform_window_id: i32,
    visual: Vec<AvoidArea>,
    keyboard: AvoidArea,
) -> Result<()> {
    if let Ok(host) = crate::app::session::current() {
        let visual = [
            visual.first().copied().unwrap_or_default(),
            visual.get(1).copied().unwrap_or_default(),
            visual.get(2).copied().unwrap_or_default(),
        ];
        host.update_window(&render_owner, platform_window_id, visual, keyboard)?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_open_url_result(open_id: u32, opened: bool) {
    if let Ok(host) = crate::app::session::current() {
        host.complete_open_url(open_id, opened);
    }
}

#[napi]
pub(crate) fn nsh_report_host_error(message: String) {
    if let Ok(host) = crate::app::session::current() {
        host.broadcast(|| Message::Error(message.clone()));
    }
}

#[napi]
pub(crate) fn dispose_all_renders(_env: Env) {
    UI_OWNER.with(|owner| {
        if let Some(owner) = owner.borrow_mut().take() {
            crate::app::session::set_ui_batch_applier(None);
            if let Ok(host) = crate::app::session::current() {
                let _ = host.set_ui_ready(false);
                host.clear_render_window(&owner);
            }
        }
    });
    crate::app::surface::dispose_all();
}

#[napi]
pub(crate) fn on_back_press_intercept() -> bool {
    crate::app::session::current().is_ok_and(|host| host.handle_back())
}
