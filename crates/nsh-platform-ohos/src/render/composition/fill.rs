//! Fill layers on the surface's compositor context and VSync.
use std::sync::Arc;

use nsh_core::{NodeId, animation::AnimationValue, shader::ShaderLayer};
use ohos_native_window_binding::NativeWindow;
use rustc_hash::FxHashMap;

use super::{
    gles::Effects,
    readback::{TextureContext, Window},
};
use crate::render::geometry::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub(crate) id: NodeId,
    pub(crate) generation: u32,
}

pub(crate) enum Command {
    Layer(Key, Arc<ShaderLayer>),
    Slot(Key, u16, AnimationValue),
    Size(Key, Size, f32),
    Shown(Key, bool),
    Window(Key, NativeWindow),
    WindowGone(Key, usize),
    Remove(Key),
}

struct View {
    effects: Effects,
    window: Option<Window>,
    size: (u32, u32),
    scale: f32,
    shown: bool,
    dirty: bool,
}
impl View {
    fn due(&self) -> bool {
        self.shown
            && self.window.is_some()
            && self.size.0 > 0
            && self.size.1 > 0
            && (self.dirty || self.effects.wants_frame())
    }
    fn render(&mut self) -> Result<(), String> {
        if self
            .effects
            .render_fill(self.size, self.scale, self.dirty)?
            && let Some(output) = self.effects.output()
            && let Some(window) = &mut self.window
        {
            output.present(window)?;
        }
        self.dirty = false;
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Fills {
    views: FxHashMap<Key, View>,
}
impl Fills {
    pub(super) fn remove(&mut self, key: Key) {
        self.views.remove(&key);
    }
    pub(super) fn apply(&mut self, command: Command, gpu: &TextureContext) -> Result<(), String> {
        match command {
            Command::Layer(key, layer) => {
                let view = self.views.entry(key).or_insert_with(|| View {
                    effects: Effects::new(gpu),
                    window: None,
                    size: (0, 0),
                    scale: 1.0,
                    shown: false,
                    dirty: true,
                });
                view.effects.layers(&[(key.id, layer)])?;
                view.dirty = true;
            }
            Command::Slot(key, slot, value) => {
                if let Some(view) = self.views.get_mut(&key) {
                    view.effects.slot(key.id, slot, value);
                }
            }
            Command::Size(key, size, scale) => {
                if let Some(view) = self.views.get_mut(&key) {
                    view.size = (
                        (size.width * scale).ceil().clamp(0.0, 4096.0) as u32,
                        (size.height * scale).ceil().clamp(0.0, 4096.0) as u32,
                    );
                    view.scale = scale;
                    view.dirty = true;
                }
            }
            Command::Shown(key, shown) => {
                if let Some(view) = self.views.get_mut(&key) {
                    view.shown = shown;
                    view.dirty |= shown;
                }
            }
            Command::Window(key, window) => {
                if let Some(view) = self.views.get_mut(&key) {
                    view.window = Some(gpu.window(window).map_err(|e| e.to_string())?);
                    view.dirty = true;
                }
            }
            Command::WindowGone(key, raw) => {
                if let Some(view) = self.views.get_mut(&key)
                    && view.window.as_ref().is_some_and(|w| w.raw() == raw)
                {
                    view.window = None;
                }
            }
            Command::Remove(key) => self.remove(key),
        }
        Ok(())
    }
    pub(super) fn render(&mut self) {
        for view in self.views.values_mut().filter(|view| view.due()) {
            if let Err(error) = view.render() {
                log::warn!("shader fill: {error}");
                view.shown = false;
            }
        }
    }
    pub(super) fn animating(&self) -> bool {
        self.views.values().any(View::due)
    }
}
