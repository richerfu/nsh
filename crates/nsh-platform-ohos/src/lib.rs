//! OHOS native backend: independent ContentSlot views in a standard UIAbility host.
//! Each rendering surface owns one engine and ArkUI tree.
//!
//! The engine lays out text off the UI thread and transfers owned native drawing
//! commands with selection geometry. Native views
//! consume one mount batch per wake, with no per-node ArkTS calls.
#![cfg(target_env = "ohos")]

mod accessibility;
mod app;
mod input;
mod interop;
mod media;
mod navigation;
mod render;
mod scroll;
mod services;
mod text;

pub use app::{
    NshCacheOptions, NshCapabilities, NshEnvironment, NshOptions, NshPackage, NshPackageInfo,
    NshSurface, NshTrust, get_capabilities, init_logging,
};
pub use interop::NshNativeProperty;

static UI_SETS: std::sync::RwLock<Vec<nsh_core::element::Registry>> =
    std::sync::RwLock::new(Vec::new());

/// Adds the app's Rust UI sets to `nsh/base` for subsequently created surfaces.
/// Call this before loading a module that imports those sets.
pub fn set_ui_sets(sets: Vec<nsh_core::element::Registry>) {
    *UI_SETS.write().unwrap_or_else(|error| error.into_inner()) = sets;
}

fn ui_sets() -> Vec<nsh_core::element::Registry> {
    let mut sets = vec![nsh_base::registry()];
    sets.extend(
        UI_SETS
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned(),
    );
    sets
}
