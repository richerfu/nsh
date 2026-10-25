//! Retained stacks and native headers. Back decisions use mounted props;
//! dismissed screens stay hidden until the engine removes their routes.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use nsh_core::{EventKind, EventPayload, ScreenAnimation, ScreenConfig, ViewKind};
use ohos_arkui_binding::{
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::{Stack, Text},
    },
    types::{advanced::LengthMetricUnit, attribute::ArkUINodeAttributeType as A},
};

use crate::{
    navigation::motion::Motion,
    render::node::{NativeNode, Node, Result},
    text::typography::argb,
};

pub(crate) struct Screen {
    owner: Weak<Node>,
    pub(crate) config: RefCell<ScreenConfig>,
    pub(crate) popped: Cell<bool>,
    header: NativeNode,
    title: NativeNode,
    back: NativeNode,
    has_back: Cell<bool>,
    top: Cell<bool>,
    revealed: Cell<bool>,
    entered: Cell<bool>,
    progress: Cell<f32>,
    peer: RefCell<Weak<Node>>,
    motion: RefCell<Option<Motion>>,
    immediate_removal: Cell<bool>,
}
impl Screen {
    pub(crate) fn new(owner: Weak<Node>) -> Result<Rc<Self>> {
        let header = Stack::new()?.into_node();
        let title = Text::new()?.into_node();
        let back = Text::new()?.into_node();
        for node in [&header, &title, &back] {
            node.borrow().set_length_metric_unit(LengthMetricUnit::Vp)?;
        }
        header.borrow_mut().add_child(back.clone())?;
        header.borrow_mut().add_child(title.clone())?;
        back.borrow().set_string_attribute(A::TextContent, "‹")?;
        back.borrow().set_f32_attribute(A::FontSize, 28.0)?;
        back.borrow().id("nsh-header-back")?;
        back.borrow().width(44.0)?;
        back.borrow().height(44.0)?;
        title.borrow().set_f32_attribute(A::FontSize, 18.0)?;
        title.borrow().set_i32_attribute(A::TextAlign, 1)?;
        title.borrow().height(44.0)?;
        let screen = Rc::new(Self {
            owner,
            config: RefCell::default(),
            popped: Cell::new(false),
            header,
            title,
            back,
            has_back: Cell::new(false),
            top: Cell::new(false),
            revealed: Cell::new(false),
            entered: Cell::new(false),
            progress: Cell::new(0.0),
            peer: RefCell::default(),
            motion: RefCell::default(),
            immediate_removal: Cell::new(false),
        });
        let weak = Rc::downgrade(&screen);
        screen.back.borrow_mut().on_click(move || {
            if let Some(screen) = weak.upgrade() {
                crate::render::node::callback(|| screen.request_back());
            }
        });
        Ok(screen)
    }
    pub(crate) fn is_top(&self) -> bool {
        self.top.get() && !self.popped.get()
    }
    pub(crate) fn paint_visible(&self) -> bool {
        self.is_top() || self.revealed.get() || self.motion.borrow().is_some()
    }
    pub(crate) fn configure(&self, config: ScreenConfig) -> Result<()> {
        *self.config.borrow_mut() = config;
        self.update(self.has_back.get())
    }
    pub(crate) fn update_environment(&self) -> Result<()> {
        self.update(self.has_back.get())
    }
    pub(crate) fn update(&self, has_back: bool) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        self.has_back.set(has_back);
        let c = self.config.borrow();
        let dark = owner
            .appearance
            .get()
            .resolve(owner.platform.get().color_scheme)
            == nsh_engine::ColorScheme::Dark;
        let fg = if dark { 0xffeeeeee } else { 0xff161616 };
        let overlap = owner.top_overlap();
        let height = if c.header_visible {
            52.0 + overlap
        } else {
            0.0
        };
        if !c.large_title && !c.header_transparent && (c.header_height.0 - height).abs() > 0.01 {
            owner
                .control
                .set_screen_header(owner.id, Some(nsh_core::Points(height)));
        }
        let h = self.header.borrow();
        h.width(owner.frame.get().width)?;
        h.height(height)?;
        h.set_i32_attribute(A::Visibility, i32::from(!c.header_visible))?;
        h.background_color(if c.header_transparent {
            0
        } else {
            c.background_color
                .map(argb)
                .unwrap_or(if dark { 0xff202020 } else { 0xfff8f8f8 })
        })?;
        self.title
            .borrow()
            .set_string_attribute(A::TextContent, c.title.as_deref().unwrap_or(""))?;
        self.title
            .borrow()
            .set_u32_attribute(A::FontColor, c.title_color.map(argb).unwrap_or(fg))?;
        self.title
            .borrow()
            .width((owner.frame.get().width - 112.0).max(0.0))?;
        self.title
            .borrow()
            .set_position(vec![56.0, 4.0 + overlap])?;
        self.title
            .borrow()
            .set_i32_attribute(A::Visibility, i32::from(c.title_view.is_some()))?;
        self.back.borrow().set_position(vec![4.0, 4.0 + overlap])?;
        self.back
            .borrow()
            .set_u32_attribute(A::FontColor, c.tint_color.map(argb).unwrap_or(fg))?;
        self.back
            .borrow()
            .set_i32_attribute(A::Visibility, i32::from(!has_back || !c.back_visible))?;
        Ok(())
    }
    pub(crate) fn request_back(&self) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        if !self.has_back.get() || self.popped.get() {
            return Ok(());
        }
        if self.config.borrow().prevent_back {
            owner.emit(EventKind::BackRequest, EventPayload::None);
        } else {
            self.popped.set(true);
            self.pop()?;
            if let Some(dispatcher) = owner.dispatcher.upgrade() {
                dispatcher.send(crate::app::runtime::Message::ReconcileNavigation);
            }
            owner.emit(EventKind::Dismiss, EventPayload::None);
        }
        Ok(())
    }
    pub(crate) fn owner(&self) -> Option<Rc<Node>> {
        self.owner.upgrade()
    }
    pub(crate) fn previous(&self) -> Option<Rc<Node>> {
        let owner = self.owner.upgrade()?;
        let parent = owner.parent_node.borrow().upgrade()?;
        let mut previous = None;
        for child in parent.children.borrow().iter().filter_map(Weak::upgrade) {
            if child.id == owner.id {
                return previous;
            }
            if child.kind == ViewKind::Screen
                && !child.leaving.get()
                && child
                    .screen
                    .borrow()
                    .as_ref()
                    .is_some_and(|screen| !screen.popped.get())
            {
                previous = Some(child);
            }
        }
        None
    }
    fn show(&self) -> Result<()> {
        if let Some(owner) = self.owner.upgrade() {
            owner.mounted().borrow().set_i32_attribute(
                A::Visibility,
                i32::from(
                    !(self.is_top() || self.revealed.get() || self.motion.borrow().is_some()),
                ),
            )?;
        }
        Ok(())
    }
    fn reveal(&self) -> Result<bool> {
        let Some(previous) = self.previous() else {
            return Ok(false);
        };
        if let Some(screen) = previous.screen.borrow().as_ref() {
            // A new push can interrupt the preceding push at its settled position.
            screen.settle()?;
            screen.revealed.set(true);
            screen.show()?;
        }
        *self.peer.borrow_mut() = Rc::downgrade(&previous);
        Ok(true)
    }
    fn paint(&self) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        let p = self.progress.get();
        let fade = self.config.borrow().animation == ScreenAnimation::Fade
            || owner.platform.get().reduce_motion;
        let sign = if owner.platform.get().rtl { -1.0 } else { 1.0 };
        let (enter_x, _) =
            nsh_core::navigation::push_offsets(nsh_core::Points(owner.frame.get().width), 1.0 - p);
        owner.navigation_presentation(
            if fade { 1.0 - p } else { 1.0 },
            if fade { 0.0 } else { enter_x.0 * sign },
        )?;
        if let Some(peer) = self.peer.borrow().upgrade() {
            let (_, below_x) = nsh_core::navigation::push_offsets(
                nsh_core::Points(peer.frame.get().width),
                1.0 - p,
            );
            peer.navigation_presentation(1.0, if fade { 0.0 } else { below_x.0 * sign })?;
        }
        Ok(())
    }
    fn animate(&self, to: f32) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        let animated = self.config.borrow().animation != ScreenAnimation::None
            && !owner.platform.get().reduce_motion;
        *self.motion.borrow_mut() = Some(Motion::new(
            owner.animator.now(),
            self.progress.get(),
            to,
            animated,
        ));
        self.show()?;
        self.paint()?;
        owner.animator.track(&owner);
        Ok(())
    }
    fn push(&self) -> Result<()> {
        if self.config.borrow().animation != ScreenAnimation::None && self.reveal()? {
            self.progress.set(1.0);
            self.animate(0.0)?;
        }
        Ok(())
    }
    fn pop(&self) -> Result<bool> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(false);
        };
        if self.immediate_removal.get() {
            return Ok(false);
        }
        if self
            .motion
            .borrow()
            .as_ref()
            .is_some_and(|motion| motion.to == 1.0)
        {
            return Ok(true);
        }
        if self.config.borrow().animation == ScreenAnimation::None || !self.reveal()? {
            self.show()?;
            return Ok(false);
        }
        owner.leaving.set(true);
        self.animate(1.0)?;
        Ok(true)
    }
    pub(crate) fn remove(&self) -> Result<bool> {
        if self.immediate_removal.get() {
            return Ok(false);
        }
        if !self.is_top() {
            return Ok(false);
        }
        self.popped.set(true);
        self.pop()
    }
    pub(crate) fn prepare_removal(&self, animated: bool) -> Result<()> {
        self.immediate_removal.set(!animated);
        if !animated {
            self.settle()?;
            if let Some(owner) = self.owner.upgrade() {
                owner.leaving.set(false);
            }
        }
        Ok(())
    }
    fn settle(&self) -> Result<()> {
        self.motion.borrow_mut().take();
        self.progress.set(0.0);
        if let Some(owner) = self.owner.upgrade() {
            owner.navigation_presentation(1.0, 0.0)?;
        }
        if let Some(peer) = self.peer.replace(Weak::new()).upgrade() {
            peer.navigation_presentation(1.0, 0.0)?;
            if let Some(screen) = peer.screen.borrow().as_ref() {
                screen.revealed.set(false);
                screen.show()?;
            }
        }
        self.show()
    }
    pub(crate) fn frame(&self, now: f64) -> Result<(bool, bool)> {
        let state = self.motion.borrow().as_ref().map(|motion| {
            let reduced = self
                .owner
                .upgrade()
                .is_some_and(|owner| owner.platform.get().reduce_motion);
            motion.step(now, reduced)
        });
        let Some((progress, done, to)) = state else {
            return Ok((false, false));
        };
        self.progress.set(progress);
        self.paint()?;
        if done {
            self.settle()?;
        }
        Ok((!done, done && to == 1.0))
    }
    pub(crate) fn can_swipe(&self, x: f32) -> bool {
        let Some(owner) = self.owner.upgrade() else {
            return false;
        };
        let c = self.config.borrow();
        self.is_top()
            && self.has_back.get()
            && c.gesture_enabled
            && !c.prevent_back
            && self.motion.borrow().is_none()
            && !owner.platform.get().screen_reader
            && if owner.platform.get().rtl {
                x >= owner.frame.get().width - 24.0
            } else {
                x <= 24.0
            }
    }
    pub(crate) fn swipe(&self, dx: f32) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        if self.peer.borrow().upgrade().is_none() {
            self.reveal()?;
        }
        self.progress
            .set((dx / owner.frame.get().width.max(1.0)).clamp(0.0, 1.0));
        self.paint()
    }
    pub(crate) fn finish_swipe(&self, complete: bool) -> Result<()> {
        if complete
            && self.is_top()
            && self.config.borrow().gesture_enabled
            && !self.config.borrow().prevent_back
        {
            self.request_back()
        } else {
            self.animate(0.0)
        }
    }
    pub(crate) fn measure(
        &self,
        event: &ohos_arkui_binding::api::node_custom_event::NodeCustomEvent,
    ) -> Result<()> {
        event.measure_child(&self.header.borrow())?;
        Ok(())
    }
    pub(crate) fn layout(
        &self,
        event: &ohos_arkui_binding::api::node_custom_event::NodeCustomEvent,
    ) -> Result<()> {
        event.layout_child(&self.header.borrow(), 0, 0)?;
        if let Some(owner) = self.owner.upgrade() {
            let c = self.config.borrow();
            let width = owner.frame.get().width;
            for child in owner
                .children
                .borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|n| n.kind == ViewKind::HeaderItem)
            {
                let frame = child.frame.get();
                let x = if c.left == Some(child.id) {
                    if self.has_back.get() { 56.0 } else { 12.0 }
                } else if c.right == Some(child.id) {
                    width - frame.width - 12.0
                } else if c.title_view == Some(child.id) {
                    (width - frame.width) * 0.5
                } else {
                    continue;
                };
                let y = owner.top_overlap() + (52.0 - frame.height).max(0.0) * 0.5;
                event.layout_child(
                    &child.mounted().borrow(),
                    (x * owner.scale).round() as i32,
                    (y * owner.scale).round() as i32,
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn install(&self, owner: &Node) -> Result<()> {
        owner.native.borrow_mut().add_child(self.header.clone())?;
        Ok(())
    }
}
pub(crate) fn reconcile(nodes: &[Option<Rc<Node>>]) -> Result<()> {
    for stack in nodes
        .iter()
        .flatten()
        .filter(|node| node.kind == ViewKind::Stack)
    {
        let screens: Vec<_> = stack
            .children
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|node| {
                node.screen
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| !s.popped.get())
            })
            .collect();
        let previous_top = screens
            .iter()
            .find(|node| node.screen.borrow().as_ref().is_some_and(|s| s.is_top()))
            .map(|node| node.id);
        for (index, screen) in screens.iter().enumerate() {
            if let Some(state) = screen.screen.borrow().as_ref() {
                state.top.set(index + 1 == screens.len());
                state.update(index > 0)?;
                if index + 1 == screens.len()
                    && !state.entered.replace(true)
                    && index > 0
                    && previous_top.is_some_and(|id| id != screen.id)
                {
                    state.push()?;
                }
                state.show()?;
            }
        }
    }
    Ok(())
}
