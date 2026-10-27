//! Main-thread ArkUI nodes. Callbacks hold weak references; the mounter owns
//! the nodes through detachment and frame-delayed retirement.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use nsh_core::{EventKind, EventMask, EventPayload, NodeId, Points, ViewKind, ViewStyle};
use ohos_arkui_binding::{
    api::node_custom_event::NodeCustomEvent,
    common::{attribute::ArkUINodeAttributeNumber as Number, node::ArkUINode},
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::Custom,
    },
    types::{
        advanced::{LengthMetricUnit, NodeDirtyFlag},
        attribute::ArkUINodeAttributeType as Attribute,
    },
};
use ohos_drawing_binding::{Bitmap, BitmapFormat, Canvas};

use crate::{
    input::{editor::Input, events::Interaction},
    render::geometry::{Rect, Size},
    scroll::controller::Scroller,
    text::typography::{RasterText, argb},
};

pub(crate) type NativeNode = Rc<RefCell<ArkUINode>>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("invalid OHOS mount node {0}")]
    InvalidNode(NodeId),
    #[error("{0} at {1}")]
    ArkUi(
        ohos_arkui_binding::common::error::ArkUIError,
        &'static std::panic::Location<'static>,
    ),
    #[cfg(feature = "api-23")]
    #[error(transparent)]
    Accessibility(#[from] ohos_accessibility_binding::AccessibilityError),
    #[error(transparent)]
    Host(#[from] napi_ohos::Error),
    #[error("OHOS native VSync request failed: {0}")]
    Vsync(i32),
    #[error("OHOS backend does not support {0}")]
    Unsupported(&'static str),
}
impl From<ohos_arkui_binding::common::error::ArkUIError> for Error {
    #[track_caller]
    fn from(err: ohos_arkui_binding::common::error::ArkUIError) -> Self {
        Self::ArkUi(err, std::panic::Location::caller())
    }
}
pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy)]
struct AppliedStyle {
    visual_ptr: usize,
    background_color: u32,
    border_radius: [f32; 4],
    border_width: [f32; 4],
    border_color: [u32; 4],
    border_style: i32,
    shadow: Option<(f32, f32, f32, u32)>,
    ink_outset: f32,
    clip: bool,
    visibility: i32,
    transform: Option<[f32; 16]>,
    transform_center: [f32; 3],
}
impl Default for AppliedStyle {
    fn default() -> Self {
        Self {
            visual_ptr: 0,
            background_color: 0,
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: [0; 4],
            border_style: 0,
            shadow: None,
            ink_outset: 0.0,
            clip: false,
            visibility: 0,
            transform: None,
            transform_center: [f32::NAN; 3],
        }
    }
}

pub(crate) struct Node {
    pub(crate) accessibility_state: crate::accessibility::state::State,
    #[cfg(feature = "api-23")]
    pub(crate) accessibility_provider: crate::accessibility::provider::State,
    pub(crate) native: NativeNode,
    pub(crate) wrapper: RefCell<Option<crate::interop::view::Wrapper>>,
    pub(crate) mount_parent: RefCell<Weak<RefCell<ArkUINode>>>,
    pub(crate) id: NodeId,
    pub(crate) kind: ViewKind,
    pub(crate) pending_delete: Cell<bool>,
    pub(crate) scene_prepared: Cell<bool>,
    pub(crate) modal: RefCell<Option<Rc<crate::navigation::modal::Modal>>>,
    pub(crate) screen: RefCell<Option<Rc<crate::navigation::stack::Screen>>>,
    pub(crate) frame: Cell<Rect>,
    pub(crate) parent: Cell<Option<NodeId>>,
    pub(crate) parent_node: RefCell<Weak<Node>>,
    pub(crate) children: RefCell<Vec<Weak<Node>>>,
    context: Rc<crate::render::mount::UiContext>,
    pub(crate) input: RefCell<Option<Rc<Input>>>,
    pub(crate) scroll: RefCell<Option<Rc<Scroller>>>,
    pub(crate) canvas: RefCell<Option<crate::render::canvas::CanvasView>>,
    pub(crate) image: RefCell<Option<Rc<crate::media::image::ImageView>>>,
    pub(crate) component: RefCell<Option<crate::interop::component::Instance>>,
    pub(crate) video: RefCell<Option<crate::media::video::Video>>,
    pub(crate) web: RefCell<Option<crate::media::webview::WebView>>,
    active: Cell<bool>,
    backface_away: Cell<bool>,
    pub(crate) interaction: Interaction,
    pub(crate) style: RefCell<ViewStyle>,
    applied: Cell<AppliedStyle>,
    styled: Cell<bool>,
    pub(crate) live: RefCell<Option<Box<crate::render::transition::Live>>>,
    pub(crate) leaving: Cell<bool>,
    extra: Cell<(f32, crate::render::transition::Extra)>,
    navigation_presentation: Cell<(f32, f32)>,
    decoration: crate::render::decor::Decoration,
    outlined: Cell<bool>,
    pub(crate) bitmap: RefCell<Option<Bitmap>>,
    pub(crate) raster: RefCell<Option<nsh_core::TextLayout>>,
    pub(crate) selection: crate::text::selection::state::State,
    tint: Cell<Option<nsh_core::Color>>,
    pub(crate) content: Cell<Rect>,
    pub(crate) overflow: Cell<Option<Rect>>,
    raster_origin_y: Cell<f32>,
    presentation: Cell<(f32, f32)>,
    pinned: Cell<(f32, f32)>,
    input_transform: Cell<nsh_core::Matrix>,
}

impl std::ops::Deref for Node {
    type Target = crate::render::mount::UiContext;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Node {
    pub(crate) fn new(
        id: NodeId,
        kind: ViewKind,
        context: Rc<crate::render::mount::UiContext>,
    ) -> Result<Rc<Self>> {
        let native = Custom::new()?.into_node();
        native
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        let visual_ptr = Rc::as_ptr(&native) as usize;
        let node = Rc::new(Self {
            accessibility_state: crate::accessibility::state::State::default(),
            #[cfg(feature = "api-23")]
            accessibility_provider: crate::accessibility::provider::State::default(),
            native,
            wrapper: RefCell::default(),
            mount_parent: RefCell::default(),
            id,
            kind,
            pending_delete: Cell::new(false),
            scene_prepared: Cell::new(false),
            modal: RefCell::new(None),
            screen: RefCell::new(None),
            frame: Cell::new(Rect::default()),
            parent: Cell::new(None),
            parent_node: RefCell::default(),
            children: RefCell::new(Vec::new()),
            context,
            input: RefCell::new(None),
            scroll: RefCell::new(None),
            image: RefCell::new(None),
            canvas: RefCell::new(None),
            component: RefCell::new(None),
            web: RefCell::default(),
            video: RefCell::default(),
            active: Cell::new(true),
            backface_away: Cell::new(false),
            interaction: Interaction::default(),
            style: RefCell::new(ViewStyle::default()),
            applied: Cell::new(AppliedStyle {
                visual_ptr,
                ..AppliedStyle::default()
            }),
            styled: Cell::new(false),
            live: RefCell::default(),
            leaving: Cell::new(false),
            extra: Cell::new((1.0, crate::render::transition::Extra::NONE)),
            navigation_presentation: Cell::new((1.0, 0.0)),
            decoration: crate::render::decor::Decoration::default(),
            outlined: Cell::new(false),
            bitmap: RefCell::new(None),
            raster: RefCell::new(None),
            selection: Default::default(),
            tint: Cell::new(None),
            content: Cell::new(Rect::default()),
            overflow: Cell::new(None),
            raster_origin_y: Cell::new(0.0),
            presentation: Cell::new((1.0, 0.0)),
            pinned: Cell::new((0.0, 0.0)),
            input_transform: Cell::new(nsh_core::Matrix::IDENTITY),
        });
        match kind {
            ViewKind::Modal => {
                *node.modal.borrow_mut() =
                    Some(crate::navigation::modal::Modal::new(Rc::downgrade(&node))?)
            }
            ViewKind::Image => {
                *node.image.borrow_mut() = Some(crate::media::image::ImageView::new(
                    Rc::downgrade(&node),
                    node.images.clone(),
                )?)
            }
            ViewKind::Input => *node.input.borrow_mut() = Some(Input::new(Rc::downgrade(&node))?),
            ViewKind::Scroll => {
                *node.scroll.borrow_mut() = Some(Scroller::new(Rc::downgrade(&node))?)
            }
            ViewKind::Canvas => {
                *node.canvas.borrow_mut() = Some(crate::render::canvas::CanvasView::new(
                    Rc::downgrade(&node),
                    node.canvas_bytes.clone(),
                ));
            }
            ViewKind::Video => {
                *node.video.borrow_mut() =
                    Some(crate::media::video::Video::new(Rc::downgrade(&node))?);
            }
            ViewKind::WebView => {
                *node.web.borrow_mut() =
                    Some(crate::media::webview::WebView::new(Rc::downgrade(&node))?);
            }
            ViewKind::Screen => {
                let screen = crate::navigation::stack::Screen::new(Rc::downgrade(&node))?;
                screen.install(&node)?;
                *node.screen.borrow_mut() = Some(screen);
            }
            ViewKind::View
            | ViewKind::Text
            | ViewKind::Native
            | ViewKind::Stack
            | ViewKind::HeaderItem
            | ViewKind::InputAccessory => {}
        }
        let weak = Rc::downgrade(&node);
        node.native.borrow_mut().on_custom_measure(move |event| {
            callback(|| {
                if let Some(node) = weak.upgrade() {
                    node.measure(event)?;
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(&node);
        node.native.borrow_mut().on_custom_layout(move |event| {
            callback(|| {
                if let Some(node) = weak.upgrade() {
                    node.layout(event)?;
                    node.invalidate_scene();
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(&node);
        node.native.borrow_mut().on_custom_draw(move |event| {
            callback(|| {
                if let Some(node) = weak.upgrade() {
                    node.draw(event);
                }
                Ok(())
            })
        });
        node_receivers_only();
        if kind == ViewKind::Modal {
            node.ensure_wrapper()?;
        }
        Ok(node)
    }
    fn contains_focus(&self) -> bool {
        self.interaction.focused()
            || self
                .children
                .borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .any(|node| node.contains_focus())
    }
    pub(crate) fn refresh_focus_within(&self) -> Result<()> {
        self.interaction.state(
            self,
            nsh_core::tailwind::state::FOCUS_WITHIN,
            self.contains_focus(),
        )?;
        if let Some(parent) = self.parent_node.borrow().upgrade() {
            parent.refresh_focus_within()?;
        }
        Ok(())
    }
    pub(crate) fn input_to_parent(&self, parent: &Node) -> Result<nsh_core::Matrix> {
        use nsh_core::{Matrix, TransformKind as K};
        let (x, y) = if let Some(scroll) = parent.scroll.borrow().as_ref() {
            let position = self.mounted().borrow().layout_position()?;
            let content = scroll.content.borrow().layout_position_in_window()?;
            let origin = parent.native.borrow().layout_position_in_window()?;
            let offset = (
                (content.x - origin.x) as f64 / self.scale as f64 + scroll.centering().0 as f64,
                (content.y - origin.y) as f64 / self.scale as f64 + scroll.centering().1 as f64,
            );
            let matrix = self
                .input_transform
                .get()
                .mul(&Matrix::of(
                    K::TranslateX,
                    position.x as f64 / self.scale as f64,
                ))
                .mul(&Matrix::of(
                    K::TranslateY,
                    position.y as f64 / self.scale as f64,
                ))
                .mul(&Matrix::of(K::Scale, scroll.scale() as f64))
                .mul(&Matrix::of(K::TranslateX, offset.0))
                .mul(&Matrix::of(K::TranslateY, offset.1));
            return Ok(if scroll.mirrored() {
                matrix
                    .mul(&Matrix::of(K::ScaleX, -1.0))
                    .mul(&Matrix::of(K::TranslateX, parent.frame.get().width as f64))
            } else {
                matrix
            });
        } else {
            // Native containers can add layout offsets (for example a screen header).
            let origin = parent.native.borrow().layout_position_in_window()?;
            let child = self.native.borrow().layout_position_in_window()?;
            ((child.x - origin.x) as f64, (child.y - origin.y) as f64)
        };
        Ok(self
            .input_transform
            .get()
            .mul(&Matrix::of(K::TranslateX, x / self.scale as f64))
            .mul(&Matrix::of(K::TranslateY, y / self.scale as f64)))
    }
    pub(crate) fn input_in_root(&self, root: &Node) -> Result<Option<nsh_core::Matrix>> {
        if self.id == root.id {
            return Ok(Some(nsh_core::Matrix::IDENTITY));
        }
        let Some(parent) = self.parent_node.borrow().upgrade() else {
            return Ok(None);
        };
        parent
            .input_in_root(root)?
            .map(|world| self.input_to_parent(&parent).map(|m| m.mul(&world)))
            .transpose()
    }
    pub(crate) fn input_priority(&self) -> bool {
        self.pinned.get() != (0.0, 0.0)
    }
    pub(crate) fn top_overlap(&self) -> f32 {
        let mut y = self.frame.get().y;
        let mut parent = self.parent_node.borrow().upgrade();
        while let Some(node) = parent {
            if node.kind == ViewKind::Modal {
                return node
                    .modal
                    .borrow()
                    .as_ref()
                    .map_or(0.0, |modal| (modal.safe_top() - y).max(0.0));
            }
            y += node.frame.get().y;
            parent = node.parent_node.borrow().upgrade();
        }
        (self.platform.get().safe_area.top.0 - y).max(0.0)
    }
    pub(crate) fn clips_children(&self) -> bool {
        self.style.borrow().overflow_hidden || self.kind == ViewKind::Scroll
    }
    pub(crate) fn scene_active(&self) -> bool {
        self.scene
            .borrow()
            .as_ref()
            .is_some_and(|scene| scene.active())
    }
    pub(crate) fn invalidate_scene(&self) {
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.invalidate();
        }
    }
    pub(crate) fn scene_opacity(&self) -> f32 {
        self.style.borrow().opacity
            * self.presentation.get().0
            * self.extra.get().0
            * self.navigation_presentation.get().0
    }
    pub(crate) fn has_native_paint(&self) -> bool {
        let style = self.style.borrow();
        self.id == nsh_core::ROOT_ID
            || self.kind != ViewKind::View
            || !style.background_color.is_transparent()
            || style.borders.widths.any(|v| v.0 > 0.0)
            || style.shadow.is_visible()
            || style.outline.is_visible()
            || style.effects.as_ref().is_some_and(|effects| {
                !effects.background_images.is_empty() || !effects.box_shadows.is_empty()
            })
    }
    pub(crate) fn capture_bounds(
        &self,
        depth: usize,
    ) -> Result<crate::render::composition::geometry::Bounds> {
        use crate::render::composition::geometry::Bounds;
        if depth > 256 {
            return Err(Error::Unsupported(
                "compositor hierarchy exceeds 256 levels",
            ));
        }
        let frame = self.frame.get();
        let mut bounds = Bounds::new(frame.width, frame.height).own(&self.style.borrow());
        if !self.clips_children() {
            for child in self.children.borrow().iter().filter_map(Weak::upgrade) {
                if child.kind == ViewKind::Modal || child.style.borrow().hidden {
                    continue;
                }
                let child_frame = child.frame.get();
                let filters = child
                    .style
                    .borrow()
                    .effects
                    .as_ref()
                    .map(|effects| effects.filters.clone())
                    .unwrap_or_default();
                let matrix = child
                    .input_transform
                    .get()
                    .mul(&nsh_core::Matrix::of(
                        nsh_core::TransformKind::TranslateX,
                        child_frame.x as f64,
                    ))
                    .mul(&nsh_core::Matrix::of(
                        nsh_core::TransformKind::TranslateY,
                        child_frame.y as f64,
                    ));
                let child = child
                    .capture_bounds(depth + 1)?
                    .filters(&filters)
                    .transformed(matrix)
                    .ok_or(Error::Unsupported("unbounded projected compositor capture"))?;
                bounds.union(child);
            }
        }
        Ok(bounds)
    }
    pub(crate) fn header_item_visible(&self) -> bool {
        if self.kind != ViewKind::HeaderItem {
            return true;
        }
        let Some(parent) = self.parent_node.borrow().upgrade() else {
            return false;
        };
        let Some(screen) = parent.screen.borrow().as_ref().cloned() else {
            return false;
        };
        let c = screen.config.borrow();
        c.header_visible
            && (c.left == Some(self.id)
                || c.right == Some(self.id)
                || c.title_view == Some(self.id))
    }
    pub(crate) fn visible_in_tree(&self) -> bool {
        if !self.active.get()
            || self.leaving.get()
            || self.backface_away.get()
            || self.style.borrow().hidden
            || !self.header_item_visible()
        {
            return false;
        }
        if self.screen.borrow().as_ref().is_some_and(|s| !s.is_top()) {
            return false;
        }
        self.parent_node
            .borrow()
            .upgrade()
            .is_none_or(|parent| parent.visible_in_tree())
    }
    pub(crate) fn paint_visible(&self) -> bool {
        self.active.get()
            && !self.backface_away.get()
            && !self.style.borrow().hidden
            && self.header_item_visible()
            && self
                .screen
                .borrow()
                .as_ref()
                .is_none_or(|screen| screen.paint_visible())
            && self
                .parent_node
                .borrow()
                .upgrade()
                .is_none_or(|parent| parent.paint_visible())
    }
    pub(crate) fn deactivate(&self) {
        callback(|| self.selection_clear());
        self.active.set(false);
        self.invalidate_scene();
        #[cfg(feature = "api-23")]
        self.accessibility_provider.clear();
        if let Some(input) = self.input.borrow().as_ref() {
            callback(|| input.close());
        }
        if let Some(scroll) = self.scroll.borrow().as_ref()
            && let Some(plane) = scroll.overlay.borrow().as_ref()
        {
            callback(|| plane.close(self));
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref() {
            callback(|| wrapper.close(self));
        }
        if let Some(web) = self.web.borrow().as_ref() {
            callback(|| web.close());
        }
        if let Some(video) = self.video.borrow().as_ref() {
            callback(|| video.close());
        }
        if let Some(route) = self.dispatcher.upgrade() {
            let mut keyboard = route.keyboard.lock().unwrap();
            if keyboard.focused == Some(self.id) {
                keyboard.focused = None;
            }
        }
        if let Some(component) = self.component.borrow().as_ref() {
            component.deactivate();
        }
    }
    pub(crate) fn emit(&self, event: EventKind, payload: EventPayload) {
        if self.leaving.get() && event != EventKind::Dismiss {
            return;
        }
        if self.active.get() || event == EventKind::Dismiss {
            if matches!(
                event,
                EventKind::Focus | EventKind::PressDown | EventKind::Press
            ) {
                self.host.activate(self.dispatcher.clone());
            }
            if matches!(event, EventKind::Focus | EventKind::Blur) {
                if matches!(self.kind, ViewKind::Input | ViewKind::Native)
                    && let Some(route) = self.dispatcher.upgrade()
                {
                    let mut keyboard = route.keyboard.lock().unwrap();
                    if event == EventKind::Focus {
                        keyboard.focused = Some(self.id);
                    } else if keyboard.focused == Some(self.id) {
                        keyboard.focused = None;
                    }
                }
                callback(|| {
                    self.interaction.state(
                        self,
                        nsh_core::tailwind::state::FOCUSED,
                        event == EventKind::Focus,
                    )
                });
                callback(|| self.refresh_focus_within());
            }
            self.control.dispatch_event(self.id, event, payload);
        }
    }
    pub(crate) fn child_container(&self) -> NativeNode {
        self.scroll
            .borrow()
            .as_ref()
            .map_or_else(|| self.native.clone(), |scroll| scroll.content.clone())
    }
    pub(crate) fn child_index(&self, index: usize) -> usize {
        self.children.borrow()[..index]
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|child| child.kind != ViewKind::Modal)
            .count()
            + usize::from(self.component.borrow().is_some())
            + usize::from(self.screen.borrow().is_some())
            + usize::from(self.web.borrow().is_some())
            + usize::from(self.video.borrow().is_some())
    }
    pub(crate) fn attach_component(
        &self,
        component: crate::interop::component::Instance,
    ) -> Result<()> {
        if let Some(old) = self.component.borrow_mut().take() {
            self.native.borrow_mut().remove_child(0)?;
            old.dispose()?;
        }
        let frame = self.frame.get();
        component.native.borrow().width(frame.width)?;
        component.native.borrow().height(frame.height)?;
        self.native
            .borrow_mut()
            .insert_child(component.native.clone(), 0)?;
        *self.component.borrow_mut() = Some(component);
        self.native
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        Ok(())
    }
    pub(crate) fn set_events(self: &Rc<Self>, events: EventMask) -> Result<()> {
        self.interaction.events(self, events)
    }
    pub(crate) fn set_frame(&self, frame: Rect) -> Result<()> {
        let old = self.frame.replace(frame);
        if old == frame {
            return Ok(());
        }
        let size_changed = old.width != frame.width || old.height != frame.height;
        if size_changed && let Some(canvas) = self.canvas.borrow().as_ref() {
            canvas.invalidate()?;
        }
        self.update_wrapper()?;
        self.interaction.hit_region(self)?;
        if size_changed {
            if let Some(component) = self.component.borrow().as_ref() {
                component.native.borrow().width(frame.width)?;
                component.native.borrow().height(frame.height)?;
            }
            if let Some(image) = self.image.borrow().as_ref() {
                image.frame(Size::new(frame.width, frame.height))?;
            }
            if let Some(web) = self.web.borrow().as_ref() {
                web.frame(Size::new(frame.width, frame.height))?;
            }
            if let Some(video) = self.video.borrow().as_ref() {
                video.frame(Size::new(frame.width, frame.height))?;
            }
            let style = self.style.borrow();
            if !style.transform.is_identity()
                || self.extra.get().1 != crate::render::transition::Extra::NONE
            {
                self.transform(&style, |_| None)?;
            }
            if let Some(scroll) = self.scroll.borrow().as_ref() {
                scroll.viewport(Size::new(frame.width, frame.height))?;
            }
            if let Some(screen) = self.screen.borrow().as_ref() {
                screen.update(screen.has_back())?;
            }
        }
        self.native
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        Ok(())
    }
    fn navigated(&self) -> bool {
        self.id == nsh_core::ROOT_ID
            || matches!(
                self.kind,
                ViewKind::Stack
                    | ViewKind::Screen
                    | ViewKind::Modal
                    | ViewKind::HeaderItem
                    | ViewKind::InputAccessory
            )
            || self
                .parent_node
                .borrow()
                .upgrade()
                .is_some_and(|p| p.kind == ViewKind::Stack)
    }
    pub(crate) fn animated_frame(
        self: &Rc<Self>,
        frame: Rect,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<()> {
        let target = self.live.borrow().as_ref().and_then(|l| l.frame_target());
        if target.unwrap_or(self.frame.get()) == frame {
            return Ok(());
        }
        let animate = animation.filter(|_| !self.navigated() && !self.platform.get().reduce_motion);
        if animate.is_some() || self.live.borrow().is_some() {
            let moving = self
                .live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .frame(self.frame.get(), frame, animate);
            if moving {
                self.animator.track(self);
                return Ok(());
            }
        }
        self.set_frame(frame)
    }
    pub(crate) fn inserted(
        self: &Rc<Self>,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<()> {
        if self.leaving.replace(false) {
            if let Some(live) = self.live.borrow_mut().as_mut() {
                live.cancel_removal();
            }
            self.extra
                .set((1.0, crate::render::transition::Extra::NONE));
            self.interaction.pointer(
                self,
                self.parent_node
                    .borrow()
                    .upgrade()
                    .is_some_and(|p| p.interaction.blocks_children()),
            )?;
            self.restore_style()?;
        }
        if self.frame.get() != Rect::default()
            || self.navigated()
            || self.platform.get().reduce_motion
        {
            return Ok(());
        }
        if let Some(transition) = animation.and_then(|a| a.create) {
            let initial = self
                .live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .appear(transition);
            self.extra.set(initial);
            self.restore_style()?;
            self.animator.track(self);
        }
        Ok(())
    }
    pub(crate) fn removing(
        self: &Rc<Self>,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<bool> {
        if self.leaving.get() {
            return Ok(true);
        }
        let mut parent = self.parent_node.borrow().upgrade();
        while let Some(ancestor) = parent {
            if ancestor.leaving.get() {
                return Ok(true);
            }
            parent = ancestor.parent_node.borrow().upgrade();
        }
        if let Some(screen) = self.screen.borrow().as_ref()
            && screen.remove()?
        {
            return Ok(true);
        }
        if self.parent.get().is_none() || self.navigated() || self.platform.get().reduce_motion {
            return Ok(false);
        }
        let Some(transition) = animation.and_then(|a| a.delete) else {
            return Ok(false);
        };
        self.leaving.set(true);
        self.native.borrow().set_i32_attribute(
            Attribute::HitTestBehavior,
            ohos_arkui_binding::types::view::HitTestMode::None.into(),
        )?;
        self.live
            .borrow_mut()
            .get_or_insert_with(Default::default)
            .disappear(transition);
        self.animator.track(self);
        Ok(true)
    }
    pub(crate) fn tick_transition(&self, now: f64) -> Result<(bool, bool)> {
        let (navigation_active, navigation_removed) = self
            .screen
            .borrow()
            .as_ref()
            .map(|screen| screen.frame(now))
            .transpose()?
            .unwrap_or_default();
        let image_active = self
            .image
            .borrow()
            .as_ref()
            .map(|image| image.tick(now))
            .transpose()?
            .unwrap_or(false);
        let video_active = self
            .video
            .borrow()
            .as_ref()
            .map(|video| video.tick(now))
            .transpose()?
            .unwrap_or(false);
        let Some(mut live) = self.live.borrow_mut().take() else {
            return Ok((
                image_active || navigation_active || video_active,
                navigation_removed,
            ));
        };
        let step = if self.platform.get().reduce_motion {
            live.finish(&mut self.style.borrow_mut())
        } else {
            live.step(now, self.frame.get(), &mut self.style.borrow_mut())
        };
        if step.active {
            *self.live.borrow_mut() = Some(live);
        }
        if let Some(frame) = step.frame {
            self.set_frame(frame)?;
        }
        self.extra.set((step.opacity, step.extra));
        self.restore_style()?;
        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        Ok((
            step.active || image_active || navigation_active || video_active,
            step.removed || navigation_removed,
        ))
    }
    pub(crate) fn finish_removal(&self) -> Result<()> {
        self.leaving.set(false);
        self.extra
            .set((1.0, crate::render::transition::Extra::NONE));
        self.restore_style()
    }
    pub(crate) fn style(self: &Rc<Self>, style: &ViewStyle) -> Result<()> {
        if style.outline.width.0 > 0.0 && !self.outlined.replace(true) {
            let weak = Rc::downgrade(self);
            self.native
                .borrow_mut()
                .on_custom_foreground_draw(move |event| {
                    callback(|| {
                        if let Some(node) = weak.upgrade()
                            && !node.scene_active()
                            && let Some(raw) =
                                event.draw_context_in_draw().and_then(|ctx| ctx.canvas())
                        {
                            // SAFETY: ArkUI retains this drawing context for this callback.
                            let canvas = unsafe { Canvas::from_raw_borrowed(raw) };
                            let frame = node.frame.get();
                            node.decoration.outline(
                                &canvas,
                                &node.style.borrow(),
                                Size::new(frame.width, frame.height),
                                node.scale,
                            );
                        }
                        Ok(())
                    })
                });
            node_receivers_only();
        }
        if self.style.borrow().effects != style.effects
            || self.style.borrow().outline != style.outline
        {
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        let mut target = style.clone();
        if self.live.borrow().is_some() || target.transition.is_some() || target.animation.is_some()
        {
            let old = self.style.borrow().clone();
            self.live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .style(
                    &mut target,
                    old,
                    self.platform.get().reduce_motion,
                    self.styled.get(),
                );
            self.animator.track(self);
        }
        self.styled.set(true);
        *self.style.borrow_mut() = target;
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.require(self);
        }
        self.ensure_wrapper()?;
        self.restore_style()
    }
    fn sync_visual_node(&self, visual: &NativeNode) {
        let ptr = Rc::as_ptr(visual) as usize;
        let mut applied = self.applied.get();
        if applied.visual_ptr != ptr {
            applied.visual_ptr = ptr;
            applied.visibility = 0;
            applied.transform = None;
            applied.transform_center = [f32::NAN; 3];
            self.applied.set(applied);
            self.interaction.invalidate_opacity();
            self.decoration.invalidate_native();
        }
    }
    fn apply_opacity(&self, style_opacity: f32) -> Result<()> {
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        self.interaction.opacity(
            self,
            if self.scene_active() {
                1.0
            } else {
                style_opacity
                    * self.presentation.get().0
                    * self.extra.get().0
                    * self.navigation_presentation.get().0
            },
        )
    }
    fn apply_background_color(&self, color: nsh_core::Color) -> Result<()> {
        let bg = argb(color);
        let mut applied = self.applied.get();
        if applied.background_color != bg {
            self.native.borrow().background_color(bg)?;
            applied.background_color = bg;
            self.applied.set(applied);
        }
        Ok(())
    }
    fn apply_borders(&self, style: &ViewStyle) -> Result<()> {
        let corners = style
            .corner_radius
            .map(|r| r.unwrap_or(style.border_radius).0);
        let radius = [
            corners.top_left,
            corners.top_right,
            corners.bottom_left,
            corners.bottom_right,
        ];
        let widths = style.borders.widths.to_array().map(|v| v.0);
        let colors = style.borders.colors.map(argb);
        let border_style = match style.borders.style {
            nsh_core::BorderStyle::Solid => 0,
            nsh_core::BorderStyle::Dashed => 1,
            nsh_core::BorderStyle::Dotted => 2,
        };
        let mut applied = self.applied.get();
        let node = self.native.borrow();
        if applied.border_radius != radius {
            node.set_number_attribute(
                Attribute::BorderRadius,
                radius.into_iter().map(Number::Float).collect(),
            )?;
            applied.border_radius = radius;
        }
        if applied.border_width != widths {
            node.set_number_attribute(
                Attribute::BorderWidth,
                widths.into_iter().map(Number::Float).collect(),
            )?;
            applied.border_width = widths;
        }
        if applied.border_color != colors {
            node.set_number_attribute(
                Attribute::BorderColor,
                colors.into_iter().map(Number::Uint).collect(),
            )?;
            applied.border_color = colors;
        }
        if applied.border_style != border_style {
            node.set_i32_attribute(Attribute::BorderStyle, border_style)?;
            applied.border_style = border_style;
        }
        self.applied.set(applied);
        Ok(())
    }
    /// Box shadows and outlines are drawn by the node's own draw callbacks,
    /// outside its bounds; ArkUI repaints a moved or restyled node only
    /// within its draw region, so its old shadow would stay on screen. A
    /// transparent native outline as wide as that paint extends the region
    /// (ArkUI counts outlines in it) without drawing anything.
    fn apply_ink_outset(&self, style: &ViewStyle) -> Result<()> {
        let mut outset = 0.0f32;
        if style.outline.is_visible() {
            outset = outset.max(style.outline.width.0 + style.outline.offset.0);
        }
        for shadow in style
            .effects
            .iter()
            .flat_map(|effects| &effects.box_shadows)
            .filter(|shadow| !shadow.inset && shadow.is_visible())
        {
            outset = outset.max(
                shadow.spread.0
                    + shadow.blur.0.max(0.0) * 1.5
                    + shadow.offset_x.0.abs().max(shadow.offset_y.0.abs()),
            );
        }
        let outset = outset.max(0.0).ceil();
        let mut applied = self.applied.get();
        if applied.ink_outset != outset {
            let node = self.native.borrow();
            if outset > 0.0 {
                node.set_u32_attribute(Attribute::OutlineColor, 0)?;
                node.set_number_attribute(Attribute::OutlineWidth, vec![Number::Float(outset); 4])?;
            } else {
                node.reset_attribute(Attribute::OutlineWidth)?;
                node.reset_attribute(Attribute::OutlineColor)?;
            }
            applied.ink_outset = outset;
            self.applied.set(applied);
        }
        Ok(())
    }
    fn apply_shadow(&self, shadow: nsh_core::Shadow) -> Result<()> {
        let next = shadow.is_visible().then(|| {
            (
                shadow.radius.0,
                shadow.offset_x.0 * self.scale,
                shadow.offset_y.0 * self.scale,
                argb(shadow.color.with_alpha(shadow.opacity)),
            )
        });
        let mut applied = self.applied.get();
        if applied.shadow != next {
            let node = self.native.borrow();
            if let Some((radius, ox, oy, color)) = next {
                node.set_number_attribute(
                    Attribute::CustomShadow,
                    vec![
                        Number::Float(radius),
                        Number::Int(0),
                        Number::Float(ox),
                        Number::Float(oy),
                        Number::Int(0),
                        Number::Uint(color),
                        Number::Uint(0),
                    ],
                )?;
            } else {
                node.reset_attribute(Attribute::CustomShadow)?;
            }
            applied.shadow = next;
            self.applied.set(applied);
        }
        Ok(())
    }
    pub(crate) fn apply_clip(&self) -> Result<()> {
        let clip = self.clips_children();
        let mut applied = self.applied.get();
        if applied.clip != clip {
            self.native
                .borrow()
                .set_bool_attribute(Attribute::Clip, clip)?;
            applied.clip = clip;
            self.applied.set(applied);
        }
        Ok(())
    }
    fn apply_style(&self, style: &ViewStyle) -> Result<()> {
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        if let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(composite) = wrapper.composite.borrow().as_ref()
        {
            composite.update(self)?;
        }
        if let Some(image) = self.image.borrow().as_ref() {
            image.style(style)?;
        }
        self.decoration.native(self, style)?;
        self.apply_opacity(style.opacity)?;
        self.transform(style, |_| None)?;
        self.apply_background_color(style.background_color)?;
        self.apply_borders(style)?;
        self.apply_shadow(style.shadow)?;
        self.apply_ink_outset(style)?;
        self.apply_clip()?;
        Ok(())
    }
    pub(crate) fn pin(&self, x: f32, y: f32) -> Result<()> {
        if self.pinned.replace((x, y)) != (x, y) {
            self.native.borrow().set_f32_attribute(
                Attribute::ZIndex,
                if x != 0.0 || y != 0.0 {
                    1_000_000.0
                } else {
                    0.0
                },
            )?;
            self.transform(&self.style.borrow(), |_| None)?;
        }
        Ok(())
    }
    pub(crate) fn presentation(&self, alpha: f32, y: f32) -> Result<()> {
        let old = self.presentation.replace((alpha, y));
        if old != (alpha, y) {
            let style = self.style.borrow();
            if old.0 != alpha {
                self.apply_opacity(style.opacity)?;
            }
            if old.1 != y {
                self.transform(&style, |_| None)?;
            }
        }
        Ok(())
    }
    pub(crate) fn reset_text_tint(&self) -> Result<()> {
        if self.tint.take().is_some() {
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        Ok(())
    }
    pub(crate) fn restore_style(&self) -> Result<()> {
        self.apply_style(&self.style.borrow())
    }
    pub(crate) fn animate(
        &self,
        values: &[(
            nsh_core::animation::AnimationProp,
            nsh_core::animation::AnimationValue,
        )],
    ) -> Result<()> {
        use nsh_core::animation::AnimationProp as P;
        let style = self.style.borrow();
        let mut opacity = None;
        let mut bg = None;
        let mut border_color = None;
        let mut shadow = style.shadow;
        let mut has_shadow = false;
        let mut has_transform = false;
        for (prop, value) in values {
            match prop {
                P::CanvasArg(slot) => {
                    if let Some(canvas) = self.canvas.borrow().as_ref() {
                        canvas.slot(*slot, *value)?;
                    }
                }
                P::Color => {
                    let color = Some(crate::render::animation::color(*value));
                    if self.tint.replace(color) != color {
                        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
                    }
                }
                P::Opacity => opacity = Some(value.number().clamp(0.0, 1.0)),
                P::BackgroundColor => bg = Some(crate::render::animation::color(*value)),
                P::BorderColor => border_color = Some(crate::render::animation::color(*value)),
                P::ShadowColor => {
                    shadow.color = crate::render::animation::color(*value);
                    has_shadow = true;
                }
                P::ShadowOpacity => {
                    shadow.opacity = value.number().clamp(0.0, 1.0);
                    has_shadow = true;
                }
                P::ShadowRadius => {
                    shadow.radius = Points(value.number().max(0.0));
                    has_shadow = true;
                }
                P::Transform(_) => has_transform = true,
                _ => {}
            }
        }
        if let Some(opacity) = opacity {
            self.apply_opacity(opacity)?;
        }
        if let Some(bg) = bg {
            self.apply_background_color(bg)?;
        }
        if let Some(color) = border_color {
            let colors = [argb(color); 4];
            let mut applied = self.applied.get();
            if applied.border_color != colors {
                self.native.borrow().set_number_attribute(
                    Attribute::BorderColor,
                    colors.into_iter().map(Number::Uint).collect(),
                )?;
                applied.border_color = colors;
                self.applied.set(applied);
            }
        }
        if has_shadow {
            self.apply_shadow(shadow)?;
        }
        if has_transform {
            self.transform(&style, |i| {
                values
                    .iter()
                    .find_map(|(p, v)| (*p == P::Transform(i as u8)).then(|| v.number()))
            })?;
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(composite) = wrapper.composite.borrow().as_ref()
            && (opacity.is_some()
                || bg.is_some()
                || border_color.is_some()
                || has_shadow
                || has_transform)
        {
            composite.update(self)?;
        }
        if let Some(image) = self.image.borrow().as_ref()
            && (bg.is_some() || border_color.is_some())
        {
            let mut animated_style = style.clone();
            if let Some(bg) = bg {
                animated_style.background_color = bg;
            }
            if let Some(color) = border_color {
                animated_style.borders.colors.fill(color);
            }
            image.style(&animated_style)?;
        }
        Ok(())
    }
    pub(crate) fn transform(
        &self,
        style: &ViewStyle,
        value: impl Fn(usize) -> Option<f32>,
    ) -> Result<()> {
        use nsh_core::{Matrix, TransformKind as K};
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        let extra = self.extra.get().1;
        let pinned = self.pinned.get();
        let nav_x = self.navigation_presentation.get().1;
        let pres_y = self.presentation.get().1;
        let parent_mirrored = self.parent_node.borrow().upgrade().is_some_and(|parent| {
            parent
                .scroll
                .borrow()
                .as_ref()
                .is_some_and(|scroll| scroll.mirrored())
        });
        let is_identity = style.transform.ops().is_empty()
            && !parent_mirrored
            && extra == crate::render::transition::Extra::NONE
            && pinned == (0.0, 0.0)
            && nav_x == 0.0
            && pres_y == 0.0;
        let mut applied = self.applied.get();
        let native = visual.borrow();
        if is_identity {
            if applied.transform.is_some() {
                native.reset_attribute(Attribute::Transform)?;
                applied.transform = None;
            }
            self.backface_away.set(false);
            let visibility = if !self.header_item_visible() {
                2
            } else {
                i32::from(style.hidden)
            };
            if applied.visibility != visibility {
                native.set_i32_attribute(Attribute::Visibility, visibility)?;
                applied.visibility = visibility;
            }
            self.applied.set(applied);
            self.input_transform.set(Matrix::IDENTITY);
            self.invalidate_scene();
            return Ok(());
        }
        let mut m = style.transform.matrix(value);
        if parent_mirrored {
            m = Matrix::of(K::ScaleX, -1.0).mul(&m);
        }
        let m = if extra == crate::render::transition::Extra::NONE {
            m
        } else {
            Matrix::of(K::Scale, extra.scale as f64)
                .mul(&Matrix::of(K::Rotate, extra.rotate.to_degrees() as f64))
                .mul(&Matrix::of(
                    K::TranslateY,
                    (extra.shift_y.0 * self.frame.get().height) as f64,
                ))
                .mul(&m)
        };
        let mut matrix = m.0;
        matrix[12] += (pinned.0 + nav_x) as f64;
        matrix[13] += (pres_y + pinned.1) as f64;
        // ArkUI matrices translate in pixels; core matrices use surface vp.
        let mut raw_values = [0.0f32; 16];
        for (i, v) in matrix.into_iter().enumerate() {
            raw_values[i] = (v * if matches!(i, 12..=14) {
                self.scale as f64
            } else if matches!(i, 3 | 7 | 11) {
                1.0 / self.scale as f64
            } else {
                1.0
            }) as f32;
        }
        if applied.transform != Some(raw_values) {
            native.set_number_attribute(
                Attribute::Transform,
                raw_values.into_iter().map(Number::Float).collect(),
            )?;
            applied.transform = Some(raw_values);
        }
        self.backface_away
            .set(style.backface_hidden && m.0[10] < 0.0);
        let visibility = if !self.header_item_visible() {
            2
        } else {
            i32::from(style.hidden || self.backface_away.get())
        };
        if applied.visibility != visibility {
            native.set_i32_attribute(Attribute::Visibility, visibility)?;
            applied.visibility = visibility;
        }
        let frame = self.frame.get();
        let ox = style
            .transform_origin
            .x
            .to_points(
                nsh_core::geometry::AbsoluteLength::Points(Points(frame.width)),
                nsh_core::geometry::REM_SIZE,
            )
            .0;
        let oy = style
            .transform_origin
            .y
            .to_points(
                nsh_core::geometry::AbsoluteLength::Points(Points(frame.height)),
                nsh_core::geometry::REM_SIZE,
            )
            .0;
        let oz = style.transform_origin.z.0;
        let center = [ox, oy, oz];
        if applied.transform_center != center {
            native.set_attribute(Attribute::TransformCenter, vec![ox, oy, oz].into())?;
            applied.transform_center = center;
        }
        self.applied.set(applied);
        let origin = (ox as f64, oy as f64, oz as f64);
        let mut before = Matrix::IDENTITY;
        before.0[12] = -origin.0;
        before.0[13] = -origin.1;
        before.0[14] = -origin.2;
        let mut after = Matrix::IDENTITY;
        after.0[12] = origin.0;
        after.0[13] = origin.1;
        after.0[14] = origin.2;
        self.input_transform
            .set(before.mul(&Matrix(matrix)).mul(&after));
        self.invalidate_scene();
        Ok(())
    }
    pub(crate) fn navigation_presentation(&self, alpha: f32, x: f32) -> Result<()> {
        let old = self.navigation_presentation.replace((alpha, x));
        if old != (alpha, x) {
            let style = self.style.borrow();
            if old.0 != alpha {
                self.apply_opacity(style.opacity)?;
            }
            if old.1 != x {
                self.transform(&style, |_| None)?;
            }
        }
        Ok(())
    }
    pub(crate) fn text(
        self: &Rc<Self>,
        raster: &RasterText,
        content: Rect,
        layout: nsh_core::TextLayout,
    ) -> Result<()> {
        if raster.exhausted {
            log::warn!("OHOS text raster budget exhausted; keeping the surface running");
        }
        let same_layout = self
            .raster
            .borrow()
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(&old.0, &layout.0));
        if !same_layout {
            // The core retains layout/selection geometry, but the transfer pixels
            // belong exclusively to this UI bitmap after upload.
            let pixels = raster.pixels.lock().expect("text pixels").take();
            let mut slot = self.bitmap.borrow_mut();
            if let Some((pixels, _transfer_budget)) = pixels {
                let reuse = slot
                    .as_ref()
                    .is_some_and(|b| b.width() == raster.width && b.height() == raster.height);
                if !reuse {
                    *slot = Some(Bitmap::new(
                        raster.width,
                        raster.height,
                        BitmapFormat::default(),
                    ));
                }
                let bitmap = slot.as_mut().unwrap();
                if bitmap.pixels_mut().len() == pixels.len() {
                    bitmap.pixels_mut().copy_from_slice(&pixels);
                } else {
                    *slot = None;
                }
            } else {
                *slot = None;
            }
        }
        self.selection_clear()?;
        *self.raster.borrow_mut() = Some(layout);
        self.content.set(content);
        self.raster_origin_y.set(raster.origin_y);
        if raster.selection.is_some() || self.wrapper.borrow().is_some() {
            self.ensure_wrapper()?;
        }
        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        Ok(())
    }
    pub(crate) fn text_listens(&self, event: EventKind) -> bool {
        self.raster
            .borrow()
            .as_ref()
            .and_then(|layout| layout.0.downcast_ref::<RasterText>())
            .is_some_and(|raster| {
                raster
                    .links
                    .iter()
                    .any(|(link, _)| link.events.contains(event))
            })
    }
    pub(crate) fn press(&self, event: EventKind, x: f32, y: f32) {
        if !self.touches.borrow().press_allowed(self.id) {
            return;
        }
        let target = {
            let raster = self.raster.borrow();
            raster
                .as_ref()
                .and_then(|layout| layout.0.downcast_ref::<RasterText>())
                .and_then(|raster| {
                    let content = self.content.get();
                    let x = x - content.x;
                    let y = y - content.y;
                    raster
                        .links
                        .iter()
                        .filter(|(link, rects)| {
                            link.events.contains(event)
                                && rects
                                    .iter()
                                    .any(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
                        })
                        .min_by_key(|(link, _)| link.end - link.start)
                        .map(|(link, _)| link.node)
                })
        };
        let payload = self.press_payload(x, y);
        if let Some(id) = target {
            if self.active.get() {
                self.control.dispatch_event(id, event, payload);
            }
        } else if self.interaction.mask().contains(event) {
            self.emit(event, payload);
        }
    }
    pub(crate) fn press_payload(&self, x: f32, y: f32) -> EventPayload {
        let position = nsh_core::Point::new(Points(x), Points(y));
        let mut page_position = position;
        if self.id != nsh_core::ROOT_ID
            && self.kind != ViewKind::Modal
            && let Ok(node_pos) = self.native.borrow().layout_position_in_window()
        {
            let mut top = self.parent_node.borrow().upgrade();
            let mut root_origin = None;
            while let Some(parent) = top {
                if parent.id == nsh_core::ROOT_ID || parent.kind == ViewKind::Modal {
                    root_origin = parent.native.borrow().layout_position_in_window().ok();
                    break;
                }
                top = parent.parent_node.borrow().upgrade();
            }
            if let Some(origin) = root_origin {
                page_position = nsh_core::Point::new(
                    Points((node_pos.x - origin.x) as f32 / self.scale + x),
                    Points((node_pos.y - origin.y) as f32 / self.scale + y),
                );
            }
        }
        EventPayload::Press(nsh_core::PressEvent {
            position,
            page_position,
            modifiers: Default::default(),
            clicks: 1,
        })
    }
    pub(crate) fn press_feedback_allowed(&self) -> bool {
        if !self.touches.borrow().press_allowed(self.id) {
            return false;
        }
        if self.kind == ViewKind::Input {
            return true;
        }
        let mut policy = self
            .scroll
            .borrow()
            .as_ref()
            .map(|scroll| scroll.keyboard_taps());
        let mut parent = self.parent_node.borrow().upgrade();
        while policy.is_none() {
            let Some(node) = parent else { break };
            if node.kind == ViewKind::Input {
                return true;
            }
            policy = node
                .scroll
                .borrow()
                .as_ref()
                .map(|scroll| scroll.keyboard_taps());
            parent = node.parent_node.borrow().upgrade();
        }
        policy != Some(nsh_core::KeyboardTaps::Never)
            || self
                .dispatcher
                .upgrade()
                .is_none_or(|route| route.keyboard.lock().unwrap().focused.is_none())
    }
    fn measure(&self, event: &NodeCustomEvent) -> Result<()> {
        let mut frame = self.frame.get();
        if let Some(modal) = self.modal.borrow().as_ref() {
            let size = modal.measure(event)?;
            frame.width = size.width;
            frame.height = size.height;
            if self.frame.get() != frame
                && let Some(canvas) = self.canvas.borrow().as_ref()
            {
                canvas.invalidate()?;
            }
            self.frame.set(frame);
        }
        event.set_measured_size(
            (frame.width * self.scale).round() as i32,
            (frame.height * self.scale).round() as i32,
        )?;
        if let Some(input) = self.input.borrow().as_ref() {
            input.measure(event)?;
        } else if let Some(image) = self.image.borrow().as_ref() {
            event.measure_child(&image.native.borrow())?;
        } else if let Some(scroll) = self.scroll.borrow().as_ref() {
            event.measure_child(&scroll.mounted().borrow())?;
        } else {
            self.measure_children(event)?;
        }
        Ok(())
    }
    pub(crate) fn measure_children(&self, event: &NodeCustomEvent) -> Result<()> {
        if let Some(web) = self
            .web
            .borrow()
            .as_ref()
            .filter(|web| web.native().borrow().is_owned())
        {
            event.measure_child(&web.native().borrow())?;
        }
        if let Some(video) = self
            .video
            .borrow()
            .as_ref()
            .filter(|video| video.native().borrow().is_owned())
        {
            event.measure_child(&video.native().borrow())?;
        }
        if let Some(screen) = self.screen.borrow().as_ref() {
            screen.measure(event)?;
        }
        if let Some(component) = self.component.borrow().as_ref() {
            event.measure_child(&component.native.borrow())?;
        }
        for child in self.children.borrow().iter().filter_map(Weak::upgrade) {
            if !child.header_item_visible() {
                continue;
            }
            event.measure_child(&child.mounted().borrow())?;
        }
        Ok(())
    }
    fn layout(&self, event: &NodeCustomEvent) -> Result<()> {
        let position = event.position_in_layout();
        event.set_layout_position(position.x, position.y)?;
        #[cfg(feature = "api-23")]
        self.accessibility_provider.bounds(self)?;
        if let Some(input) = self.input.borrow().as_ref() {
            input.layout(event, self.scale)?;
        } else if let Some(image) = self.image.borrow().as_ref() {
            event.layout_child(&image.native.borrow(), 0, 0)?;
        } else if let Some(scroll) = self.scroll.borrow().as_ref() {
            event.layout_child(&scroll.mounted().borrow(), 0, 0)?;
        } else {
            self.layout_children(event)?;
        }
        Ok(())
    }
    pub(crate) fn layout_children(&self, event: &NodeCustomEvent) -> Result<()> {
        if let Some(web) = self
            .web
            .borrow()
            .as_ref()
            .filter(|web| web.native().borrow().is_owned())
        {
            event.layout_child(&web.native().borrow(), 0, 0)?;
        }
        if let Some(video) = self
            .video
            .borrow()
            .as_ref()
            .filter(|video| video.native().borrow().is_owned())
        {
            event.layout_child(&video.native().borrow(), 0, 0)?;
        }
        if let Some(component) = self.component.borrow().as_ref() {
            event.layout_child(&component.native.borrow(), 0, 0)?;
        }
        for child in self.children.borrow().iter().filter_map(Weak::upgrade) {
            if child.kind == ViewKind::HeaderItem && self.screen.borrow().is_some() {
                continue;
            }
            let frame = child.frame.get();
            event.layout_child(
                &child.mounted().borrow(),
                (frame.x * self.scale).round() as i32,
                (frame.y * self.scale).round() as i32,
            )?;
        }
        if let Some(screen) = self.screen.borrow().as_ref() {
            screen.layout(event)?;
        }
        Ok(())
    }
    fn draw(&self, event: &NodeCustomEvent) {
        let has_effects = self
            .style
            .borrow()
            .effects
            .as_ref()
            .is_some_and(|e| !e.background_images.is_empty() || !e.box_shadows.is_empty());
        if !has_effects {
            self.decoration.clear_paint_cache();
            if self.canvas.borrow().is_none()
                && self.bitmap.borrow().is_none()
                && self.selection.range.get().is_none()
            {
                return;
            }
        }
        if let Some(raw) = event.draw_context_in_draw().and_then(|ctx| ctx.canvas()) {
            // SAFETY: ArkUI owns this canvas for the duration of this draw callback.
            let canvas = unsafe { Canvas::from_raw_borrowed(raw) };
            if has_effects {
                let frame = self.frame.get();
                self.decoration.background(
                    &canvas,
                    &self.style.borrow(),
                    Size::new(frame.width, frame.height),
                    self.scale,
                );
            }
            if let Some(drawing) = self.canvas.borrow().as_ref() {
                callback(|| drawing.draw(&canvas));
            }
            self.draw_selection(&canvas, false);
            if let Some(bitmap) = self.bitmap.borrow().as_ref() {
                let content = self.content.get();
                let brush = self
                    .tint
                    .get()
                    .and_then(|color| {
                        ohos_drawing_binding::ColorFilter::blend(
                            argb(color),
                            ohos_drawing_binding::BlendMode::SrcIn,
                        )
                    })
                    .map(|color| {
                        let mut filter = ohos_drawing_binding::Filter::new();
                        filter.set_color_filter(Some(&color));
                        let mut brush = ohos_drawing_binding::Brush::new();
                        brush.set_filter(Some(&filter));
                        brush
                    });
                if let Some(brush) = &brush {
                    canvas.attach_brush(brush);
                }
                let raster_scale = self
                    .raster
                    .borrow()
                    .as_ref()
                    .and_then(|layout| layout.0.downcast_ref::<RasterText>())
                    .map_or(self.scale, |raster| raster.scale);
                canvas.save();
                canvas.translate(
                    content.x * self.scale,
                    (content.y + self.raster_origin_y.get()) * self.scale,
                );
                canvas.scale(self.scale / raster_scale, self.scale / raster_scale);
                canvas.draw_bitmap(bitmap, 0.0, 0.0);
                canvas.restore();
                if brush.is_some() {
                    canvas.detach_brush();
                }
            }
            self.draw_selection(&canvas, true);
        }
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        // Struct fields otherwise outlive this Drop body. Release controller
        // handles before recursively disposing their mounted native descendants.
        #[cfg(feature = "api-23")]
        self.accessibility_provider.clear();
        if let Some(input) = self.input.borrow_mut().take() {
            callback(|| input.close());
        }
        if let Some(web) = self.web.borrow_mut().take() {
            callback(|| web.close());
            let native = web.native().clone();
            drop(web);
            if native.borrow().is_owned() {
                callback(|| {
                    let index = self
                        .native
                        .borrow()
                        .children()
                        .iter()
                        .position(|child| Rc::ptr_eq(child, &native));
                    if let Some(index) = index {
                        if native.borrow().parent()?.is_some() {
                            self.native.borrow_mut().remove_child(index)?;
                        } else {
                            self.native.borrow_mut().children_mut().remove(index);
                        }
                    } else {
                        native.borrow().remove_from_parent()?;
                    }
                    if Rc::strong_count(&native) == 1 {
                        native.borrow_mut().dispose()?;
                    }
                    Ok(())
                });
            }
        }
        if let Some(video) = self.video.borrow_mut().take() {
            callback(|| video.close());
        }
        callback(|| self.interaction.dispose(self));
        if let Some(scroll) = self.scroll.borrow_mut().take() {
            callback(|| scroll.dispose());
            if let Some(plane) = scroll.overlay.borrow_mut().take() {
                callback(|| plane.close(self));
                // Its outer node remains in self.native; dropping Plane
                // releases its duplicate handle and its inner wrapper handle.
            }
            let wrapper = scroll.wrapper.clone();
            drop(scroll);
            // Texture export replaces the wrapper in the mounted tree. Once
            // the attachment releases it, this detached tree needs its own disposal.
            if wrapper.borrow().is_owned() && Rc::strong_count(&wrapper) == 1 {
                callback(|| {
                    wrapper.borrow().remove_from_parent()?;
                    wrapper.borrow_mut().dispose()?;
                    Ok(())
                });
            }
        }
        self.screen.borrow_mut().take();
        self.image.borrow_mut().take();
        if let Some(modal) = self.modal.borrow_mut().take() {
            callback(|| modal.close(false));
        }
        if let Some(component) = self.component.borrow_mut().take() {
            callback(|| {
                self.native.borrow_mut().remove_child(0)?;
                component.dispose()
            });
        }
        if let Some(wrapper) = self.wrapper.borrow_mut().take() {
            callback(|| wrapper.close(self));
            if wrapper.outer.borrow().is_owned() && Rc::strong_count(&wrapper.outer) == 1 {
                callback(|| {
                    wrapper.outer.borrow().remove_from_parent()?;
                    wrapper.outer.borrow_mut().dispose()?;
                    Ok(())
                });
            }
        }
        // A retiring Video retains this parent until its worker acknowledges
        // release. That UI owner disposes the parent after detaching its child.
        if Rc::strong_count(&self.native) == 1
            && let Err(error) = self.native.borrow_mut().dispose()
        {
            log::error!("OHOS dispose: {error}");
        }
    }
}
/// Keeps custom events (measure, layout, draw) on the per-node receivers.
///
/// `ohos-arkui-binding` adds a receiver to the node *and* installs the same
/// trampoline as ArkUI's global receiver on every registration; ArkUI calls a
/// node's receivers and then the global one, so each event would reach its
/// callback twice: every layout pass ran twice and translucent paint (box
/// shadows, gradients) was drawn twice, at double opacity. Call after
/// registering custom events.
pub(crate) fn node_receivers_only() {
    use std::sync::OnceLock;
    static API: OnceLock<usize> = OnceLock::new();
    let api = *API.get_or_init(|| {
        // SAFETY: ArkUI's documented module query; the name is NUL-terminated.
        unsafe {
            ohos_arkui_sys::OH_ArkUI_QueryModuleInterfaceByName(
                ohos_arkui_sys::ArkUI_NativeAPIVariantKind_ARKUI_NATIVE_NODE,
                c"ArkUI_NativeNodeAPI_1".as_ptr().cast(),
            ) as usize
        }
    }) as *mut ohos_arkui_sys::ArkUI_NativeNodeAPI_1;
    // SAFETY: the interface table lives for the process; clearing the global
    // receiver is its documented use, on the UI thread that registered it.
    if let Some(register) =
        unsafe { api.as_ref() }.and_then(|api| api.registerNodeCustomEventReceiver)
    {
        unsafe { register(None) };
    }
}
pub(crate) fn callback(f: impl FnOnce() -> Result<()>) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::error!("OHOS callback: {error}"),
        Err(_) => log::error!("panic in an OHOS native callback"),
    }
}
