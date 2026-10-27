//! UI-thread controls and producer ownership survive worker release acknowledgement.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use nsh_core::{ResizeMode, VideoProps};
use ohos_arkui_binding::{
    common::node_registry,
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::{Button, Row, Slider, Stack, XComponent},
    },
    types::{
        advanced::LengthMetricUnit, alignment::Alignment, attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
    },
};
use ohos_native_window_binding::NativeWindow;
use ohos_xcomponent_binding::NativeXComponent;

use super::Operation;
use crate::render::{
    geometry::Size,
    node::{NativeNode, Node, Result, callback},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static LIVE:RefCell<BTreeMap<u64,Weak<Ui>>>=RefCell::default();
    static RETIRING:RefCell<BTreeMap<u64,Rc<Ui>>>=RefCell::default();
}
#[derive(Clone, Copy, Default)]
pub(crate) struct UiState {
    pub(crate) playing: bool,
    pub(crate) time: i32,
    pub(crate) duration: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}
pub(super) struct Ui {
    pub(super) token: u64,
    pub(super) native: NativeNode,
    owner: Weak<Node>,
    parent: NativeNode,
    surface: NativeNode,
    xcomponent: NativeXComponent,
    row: NativeNode,
    button: NativeNode,
    slider: NativeNode,
    closed: Cell<bool>,
    controls: Cell<bool>,
    visibility: Cell<super::controls::Controls>,
    size: Cell<Size>,
    state: Cell<UiState>,
    mode: Cell<ResizeMode>,
    scale: f32,
    updating_slider: Rc<Cell<bool>>,
}
#[derive(Default)]
struct Construction {
    nodes: Vec<NativeNode>,
    xcomponent: Option<NativeXComponent>,
    committed: bool,
}
impl Construction {
    fn own(&mut self, node: NativeNode) -> NativeNode {
        self.nodes.push(node.clone());
        node
    }
}
impl Drop for Construction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(xcomponent) = self.xcomponent.as_ref() {
            xcomponent.unregister_callbacks();
        }
        for node in &self.nodes {
            let result = (|| -> Result<()> {
                node.borrow().remove_event_receiver()?;
                node_registry::unregister(&node.borrow());
                node.borrow().remove_from_parent()?;
                Ok(())
            })();
            if let Err(error) = result {
                log::error!("Video construction cleanup failed; retaining owners: {error}");
                std::mem::forget(std::mem::take(&mut self.nodes));
                std::mem::forget(self.xcomponent.take());
                return;
            }
        }
        for node in &self.nodes {
            node.borrow_mut().children_mut().clear();
        }
        for node in self.nodes.iter().rev() {
            let owned = node.borrow().is_owned();
            if owned && let Err(error) = node.borrow_mut().dispose() {
                log::error!("Video construction node disposal: {error}");
            }
        }
    }
}
impl Ui {
    pub(super) fn new(owner: &Rc<Node>, queue: mpsc::SyncSender<Operation>) -> Result<Rc<Self>> {
        let token = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| napi_ohos::Error::from_reason("Video UI identity exhausted"))?;
        let mut construction = Construction::default();
        let native = construction.own(Stack::new()?.into_node());
        let surface = construction.own(XComponent::new()?.into_node());
        let row = construction.own(Row::new()?.into_node());
        let button = construction.own(Button::new()?.into_node());
        let slider = construction.own(Slider::new()?.into_node());
        for node in [&native, &surface, &row, &button, &slider] {
            node.borrow().set_length_metric_unit(LengthMetricUnit::Vp)?;
        }
        native
            .borrow()
            .set_i32_attribute(A::Alignment, Alignment::Center.into())?;
        native.borrow().set_bool_attribute(A::Clip, true)?;
        native
            .borrow()
            .set_u32_attribute(A::BackgroundColor, 0xff000000)?;
        surface
            .borrow()
            .set_string_attribute(A::XComponentId, format!("nsh-video-{token}"))?;
        row.borrow()
            .set_u32_attribute(A::BackgroundColor, 0x99000000)?;
        row.borrow().set_f32_attribute(A::Padding, 8.0)?;
        button
            .borrow()
            .set_string_attribute(A::ButtonLabel, "Play")?;
        button.borrow().width(80.0)?;
        row.borrow_mut().add_child(button.clone())?;
        row.borrow_mut().add_child(slider.clone())?;
        native.borrow_mut().add_child(surface.clone())?;
        let xcomponent = surface
            .borrow()
            .native_xcomponent()
            .ok_or_else(|| napi_ohos::Error::from_reason("C Video XComponent is unavailable"))?;
        construction.xcomponent = Some(xcomponent.clone());
        let send = queue.clone();
        xcomponent.on_surface_created(move |_, window| {
            let raw = std::ptr::NonNull::new(window.0).ok_or_else(|| {
                napi_ohos::Error::from_reason("Video native surface window is null")
            })?;
            // SAFETY: this callback borrows a live SDK window; the UI owner is
            // retained until the worker releases its player and acknowledges.
            let window = unsafe { NativeWindow::retain_from_ptr(raw) }
                .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))?;
            send.try_send(Operation::Window(Some(window)))
                .map_err(|_| napi_ohos::Error::from_reason("Video window queue is full or closed"))
        });
        let send = queue.clone();
        xcomponent.on_surface_destroyed(move |_, window| {
            send.try_send(Operation::WindowGone(window.0 as usize))
                .map_err(|_| napi_ohos::Error::from_reason("Video window queue is full or closed"))
        });
        xcomponent.register_callback()?;
        let send = queue.clone();
        button.borrow_mut().on_event_no_param(E::OnClick, move || {
            if let Err(error) = send.try_send(Operation::Toggle) {
                log::error!("Video control queue: {error}");
            }
        });
        let updating_slider = Rc::new(Cell::new(false));
        let setting = updating_slider.clone();
        slider
            .borrow_mut()
            .on_event(E::SliderEventOnChange, move |event| {
                if let Some(seconds) =
                    slider_seek(event.f32_value(0), event.i32_value(1), setting.get())
                    && let Err(error) = queue.try_send(Operation::Seek(seconds))
                {
                    log::error!("Video seek queue: {error}");
                }
            });
        let ui = Rc::new(Self {
            token,
            native,
            owner: Rc::downgrade(owner),
            parent: owner.native.clone(),
            surface,
            xcomponent,
            row,
            button,
            slider,
            closed: Cell::new(false),
            controls: Cell::new(false),
            visibility: Cell::new(super::controls::Controls::default()),
            size: Cell::new(Size::default()),
            state: Cell::new(UiState::default()),
            mode: Cell::new(ResizeMode::Contain),
            scale: owner.scale,
            updating_slider,
        });
        let weak = Rc::downgrade(&ui);
        ui.surface.borrow_mut().on_click(move || {
            if let Some(ui) = weak
                .upgrade()
                .filter(|ui| !ui.closed.get() && ui.controls.get())
            {
                callback(|| {
                    let mut visibility = ui.visibility.get();
                    visibility.toggle(ui.state.get().playing, ui.now());
                    ui.visibility.set(visibility);
                    ui.apply_visibility()?;
                    ui.track_controls();
                    Ok(())
                });
            }
        });
        LIVE.with_borrow_mut(|live| {
            live.insert(token, Rc::downgrade(&ui));
        });
        construction.committed = true;
        Ok(ui)
    }
    pub(super) fn abort(self: &Rc<Self>) {
        self.closed.set(true);
        LIVE.with_borrow_mut(|live| {
            live.remove(&self.token);
        });
        if let Err(error) = self.complete_retirement() {
            log::error!("Video startup cleanup failed; retaining owners: {error}");
            RETIRING.with_borrow_mut(|retiring| {
                retiring.insert(self.token, self.clone());
            });
        }
    }
    pub(super) fn current_window(&self) -> Result<Option<NativeWindow>> {
        let Some(window) = self.xcomponent.native_window() else {
            return Ok(None);
        };
        let raw = NonNull::new(window.raw())
            .ok_or_else(|| napi_ohos::Error::from_reason("Video native window is null"))?;
        // SAFETY: the XComponent registry supplies this live UI-owned surface;
        // the retained UI owner survives worker use until close acknowledgement.
        unsafe { NativeWindow::retain_from_ptr(raw) }
            .map(Some)
            .map_err(|e| napi_ohos::Error::from_reason(e.to_string()).into())
    }
    pub(super) fn configure(&self, props: &VideoProps) -> Result<()> {
        self.mode.set(props.resize_mode);
        if self.controls.get() != props.controls {
            if props.controls {
                self.native.borrow_mut().add_child(self.row.clone())?;
            } else {
                self.native.borrow_mut().remove_child(1)?;
            }
            self.controls.set(props.controls);
            if props.controls {
                let mut visibility = self.visibility.get();
                visibility.show(self.state.get().playing, self.now());
                self.visibility.set(visibility);
                self.track_controls();
            }
        }
        self.apply_visibility()?;
        self.layout()
    }
    pub(super) fn frame(&self, size: Size) -> Result<()> {
        self.size.set(size);
        self.native.borrow().width(size.width)?;
        self.native.borrow().height(size.height)?;
        self.layout()
    }
    fn now(&self) -> f64 {
        self.owner.upgrade().map_or(0.0, |node| node.animator.now())
    }
    fn track_controls(&self) {
        if self.controls.get()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.animator.track(&owner);
        }
    }
    fn apply_visibility(&self) -> Result<()> {
        self.row
            .borrow()
            .set_i32_attribute(A::Visibility, i32::from(!self.visibility.get().visible))?;
        Ok(())
    }
    pub(super) fn tick(&self, now: f64) -> Result<bool> {
        if !self.controls.get() {
            return Ok(false);
        }
        let mut visibility = self.visibility.get();
        let active = visibility.tick(now);
        self.visibility.set(visibility);
        self.apply_visibility()?;
        Ok(active)
    }
    fn layout(&self) -> Result<()> {
        let size = self.size.get();
        let state = self.state.get();
        let (width, height) = picture(
            size,
            (state.width, state.height),
            self.mode.get(),
            self.scale,
        );
        self.surface.borrow().width(width)?;
        self.surface.borrow().height(height)?;
        self.row.borrow().width(size.width)?;
        self.row.borrow().height(48.0)?;
        self.row
            .borrow()
            .set_position(vec![0.0, (size.height - 48.0).max(0.0)])?;
        self.slider.borrow().width((size.width - 104.0).max(0.0))?;
        if let Some(owner) = self.owner.upgrade() {
            let _ = owner
                .native
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure);
        }
        Ok(())
    }
    pub(super) fn begin_retirement(self: &Rc<Self>) {
        self.closed.set(true);
        LIVE.with_borrow_mut(|live| {
            live.remove(&self.token);
        });
        if self.native.borrow().is_owned() {
            RETIRING.with_borrow_mut(|retiring| {
                retiring.insert(self.token, self.clone());
            });
        }
    }
    fn complete_retirement(&self) -> Result<()> {
        self.xcomponent.unregister_callbacks();
        for node in [&self.button, &self.slider, &self.surface] {
            if node.borrow().is_owned() {
                node.borrow().remove_event_receiver()?;
                node_registry::unregister(&node.borrow());
            }
        }
        if self.native.borrow().is_owned() {
            let index = self
                .parent
                .borrow()
                .children()
                .iter()
                .position(|child| Rc::ptr_eq(child, &self.native));
            if let Some(index) = index {
                if self.native.borrow().parent()?.is_some() {
                    self.parent.borrow_mut().remove_child(index)?;
                } else {
                    self.parent.borrow_mut().children_mut().remove(index);
                }
            } else {
                self.native.borrow().remove_from_parent()?;
            }
        }
        // The controls have independent owners even when hidden. Remove both
        // native and Rust child edges before disposing any of those owners.
        for (parent, node) in [
            (&self.row, &self.button),
            (&self.row, &self.slider),
            (&self.native, &self.surface),
            (&self.native, &self.row),
        ] {
            if node.borrow().is_owned() {
                node.borrow().remove_from_parent()?;
                parent
                    .borrow_mut()
                    .children_mut()
                    .retain(|child| !Rc::ptr_eq(child, node));
            }
        }
        if self.native.borrow().is_owned() {
            self.native.borrow_mut().dispose()?;
        }
        // Independent Rc owners cover children already popped from a failed
        // parent disposal and controls currently detached from the native Stack.
        for node in [&self.surface, &self.row, &self.button, &self.slider] {
            if node.borrow().is_owned() {
                node.borrow().remove_from_parent()?;
                node.borrow_mut().dispose()?;
            }
        }
        if self.owner.upgrade().is_none()
            && self.parent.borrow().is_owned()
            && Rc::strong_count(&self.parent) == 1
        {
            self.parent.borrow_mut().dispose()?;
        }
        Ok(())
    }
}
pub(crate) fn update(token: u64, state: UiState) {
    let ui = LIVE.with_borrow(|live| live.get(&token).and_then(Weak::upgrade));
    if let Some(ui) = ui.filter(|ui| !ui.closed.get()) {
        callback(|| {
            let was_playing = ui.state.replace(state).playing;
            if was_playing != state.playing {
                let mut visibility = ui.visibility.get();
                visibility.show(state.playing, ui.now());
                ui.visibility.set(visibility);
                ui.apply_visibility()?;
                ui.track_controls();
            }
            ui.button.borrow().set_string_attribute(
                A::ButtonLabel,
                if state.playing { "Pause" } else { "Play" },
            )?;
            ui.slider
                .borrow()
                .set_f32_attribute(A::SliderMaxValue, state.duration.max(1) as f32)?;
            ui.updating_slider.set(true);
            let updated = ui
                .slider
                .borrow()
                .set_f32_attribute(A::SliderValue, state.time.max(0) as f32);
            ui.updating_slider.set(false);
            updated?;
            ui.layout()
        });
    }
}
pub(crate) fn retired(token: u64) {
    let ui = RETIRING
        .with_borrow_mut(|retiring| retiring.remove(&token))
        .or_else(|| LIVE.with_borrow(|live| live.get(&token).and_then(Weak::upgrade)));
    if let Some(ui) = ui
        && let Err(error) = ui.complete_retirement()
    {
        log::error!("Video UI release failed; retaining owner: {error}");
        RETIRING.with_borrow_mut(|retiring| {
            retiring.insert(token, ui);
        });
    }
}
// The native Slider event's state uses the SDK SliderChangeMode discriminants:
// End=2 and Click=3. Begin/Moving never commit playback seeks.
fn slider_seek(value: Option<f32>, mode: Option<i32>, programmatic: bool) -> Option<f64> {
    if programmatic || !matches!(mode, Some(2 | 3)) {
        return None;
    }
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| f64::from(v) / 1000.0)
}
fn picture(size: Size, video: (i32, i32), mode: ResizeMode, scale: f32) -> (f32, f32) {
    if video.0 <= 0 || video.1 <= 0 || mode == ResizeMode::Stretch {
        return (size.width, size.height);
    }
    let (width, height) = (video.0 as f32, video.1 as f32);
    let x = size.width / width;
    let y = size.height / height;
    let mut factor = if mode == ResizeMode::Cover {
        x.max(y)
    } else {
        x.min(y)
    };
    if mode == ResizeMode::Center {
        factor = factor.min(1.0 / scale.max(f32::MIN_POSITIVE));
    }
    (width * factor, height * factor)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_committed_user_slider_changes_seek() {
        for mode in [0, 1, 4, -1] {
            assert_eq!(slider_seek(Some(1000.0), Some(mode), false), None);
        }
        for mode in [2, 3] {
            assert_eq!(slider_seek(Some(1250.0), Some(mode), false), Some(1.25));
            assert_eq!(slider_seek(Some(1250.0), Some(mode), true), None);
        }
        for value in [f32::NAN, f32::INFINITY, -1.0] {
            assert_eq!(slider_seek(Some(value), Some(2), false), None);
        }
    }
    #[test]
    fn picture_fit_preserves_cover_contain_center_and_stretch() {
        let size = Size::new(200.0, 100.0);
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Contain, 2.0),
            (100.0, 100.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Cover, 2.0),
            (200.0, 200.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Stretch, 2.0),
            (200.0, 100.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Center, 2.0),
            (50.0, 50.0)
        );
    }
}
