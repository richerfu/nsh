//! Native interaction state and view metadata. Input feedback is answered on
//! the UI thread from mounted props; notifications to the engine are asynchronous.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use nsh_core::{
    EventKind, EventMask, EventPayload, GestureValues, PayloadValue, PointerEvents, ViewProps,
    tailwind::state,
};
use ohos_arkui_binding::{
    arkui_input_binding::UIInputAction,
    component::attribute::{ArkUICommonAttribute, ArkUIEvent, ArkUIGesture},
    gesture::{gesture_data::GestureData, inner_gesture::Gesture},
    types::{
        attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
        gesture_direction::GestureDirection,
        gesture_event::GestureEventAction,
        view::{AccessibilityMode, HitTestMode},
    },
};

use crate::render::node::{Node, Result, callback};

#[derive(Default)]
pub(crate) struct Interaction {
    props: RefCell<ViewProps>,
    events: Cell<EventMask>,
    tracked: Cell<u8>,
    states: Cell<u8>,
    inherited_block: Cell<bool>,
    click_registered: Cell<bool>,
    touch_registered: Cell<bool>,
    expanded_hit_region: Cell<bool>,
    hover_registered: Cell<bool>,
    opacity: Cell<Option<f32>>,
    label: RefCell<String>,
    long_press: RefCell<Option<(u32, Gesture)>>,
    long_fired: Cell<bool>,
    gestures: RefCell<Vec<Gesture>>,
    gesture_values: Cell<GestureValues>,
    gesture_start: Cell<[f32; 4]>,
}

impl Interaction {
    pub(crate) fn test_id(&self) -> Option<String> {
        self.props.borrow().test_id.as_deref().map(str::to_owned)
    }
    pub(crate) fn hit_region(&self, owner: &Node) -> Result<()> {
        let mut slop = self.props.borrow().hit_slop;
        if !owner.clips_children()
            && let Some(extent) = owner.overflow.get()
        {
            let frame = owner.frame.get();
            slop.left = slop.left.max(nsh_core::Points(-extent.x));
            slop.top = slop.top.max(nsh_core::Points(-extent.y));
            slop.right = slop
                .right
                .max(nsh_core::Points(extent.x + extent.width - frame.width));
            slop.bottom = slop
                .bottom
                .max(nsh_core::Points(extent.y + extent.height - frame.height));
        }
        if slop == nsh_core::EdgeInsets::default() {
            if self.expanded_hit_region.replace(false) {
                owner.native.borrow().reset_attribute(A::ResponseRegion)?;
            }
        } else {
            self.expanded_hit_region.set(true);
            let f = owner.frame.get();
            owner.native.borrow().set_attribute(
                A::ResponseRegion,
                vec![
                    -slop.left.0,
                    -slop.top.0,
                    if f.width > 0.0 {
                        (f.width + slop.left.0 + slop.right.0) / f.width
                    } else {
                        1.0
                    },
                    if f.height > 0.0 {
                        (f.height + slop.top.0 + slop.bottom.0) / f.height
                    } else {
                        1.0
                    },
                ]
                .into(),
            )?;
        }
        Ok(())
    }
    pub(crate) fn focused(&self) -> bool {
        self.states.get() & state::FOCUSED != 0
    }
    pub(crate) fn hit_slop(&self) -> nsh_core::EdgeInsets {
        self.props.borrow().hit_slop
    }
    pub(crate) fn pointer_events(&self) -> PointerEvents {
        self.props.borrow().pointer_events
    }
    pub(crate) fn pan_activation(&self) -> Option<nsh_core::PanActivation> {
        self.props.borrow().pan_activation
    }
    pub(crate) fn mask(&self) -> EventMask {
        self.events.get()
    }
    pub(crate) fn events(&self, owner: &Rc<Node>, events: EventMask) -> Result<()> {
        self.events.set(events);
        if let Some(scroll) = owner.scroll.borrow().as_ref() {
            scroll.refreshable(events.contains(EventKind::Refresh))?;
        }
        let blocked = self.inherited_block.get()
            || matches!(
                self.props.borrow().pointer_events,
                PointerEvents::None | PointerEvents::BoxNone
            );
        let click =
            !blocked && (events.contains(EventKind::Press) || owner.text_listens(EventKind::Press));
        if self.click_registered.replace(click) != click {
            if click {
                let weak = Rc::downgrade(owner);
                owner
                    .native
                    .borrow_mut()
                    .on_event(E::OnClick, move |event| {
                        if let Some(owner) = weak.upgrade()
                            && !owner.interaction.long_fired.replace(false)
                        {
                            owner.press(
                                EventKind::Press,
                                event.component_f32(0).unwrap_or_default(),
                                event.component_f32(1).unwrap_or_default(),
                            );
                        }
                    });
            } else {
                owner.native.borrow_mut().off_event(E::OnClick)?;
            }
        }
        let touch = !blocked
            && (owner.id == nsh_core::ROOT_ID
                || owner.kind == nsh_core::ViewKind::Modal
                || owner.selectable()
                || click
                || events.contains(EventKind::PressDown)
                || events.contains(EventKind::PressUp)
                || self.tracked.get() & state::PRESSED != 0);
        if self.touch_registered.replace(touch) != touch {
            if touch {
                let weak = Rc::downgrade(owner);
                owner.native.borrow_mut().on_touch_event(move |event| {
                    callback(|| {
                        if let Some(owner) = weak.upgrade()
                            && let Some(input) = event.input_event()
                        {
                            if owner.id == nsh_core::ROOT_ID
                                || owner.kind == nsh_core::ViewKind::Modal
                            {
                                owner.touches.borrow_mut().input(&owner, &input)?;
                            }
                            match input.action {
                                UIInputAction::Down => {
                                    owner.interaction.long_fired.set(false);
                                    if owner.press_feedback_allowed() {
                                        owner.interaction.state(&owner, state::PRESSED, true)?;
                                        owner.emit(EventKind::PressDown, EventPayload::None);
                                    }
                                }
                                UIInputAction::Up | UIInputAction::Cancel => {
                                    owner.interaction.state(&owner, state::PRESSED, false)?;
                                    if owner.press_feedback_allowed() {
                                        owner.emit(EventKind::PressUp, EventPayload::None);
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(())
                    })
                });
            } else {
                owner.native.borrow_mut().off_event(E::TouchEvent)?;
            }
        }
        let hover = !blocked
            && (events.contains(EventKind::HoverIn)
                || events.contains(EventKind::HoverOut)
                || self.tracked.get() & state::HOVERED != 0);
        if self.hover_registered.replace(hover) != hover {
            if hover {
                let weak = Rc::downgrade(owner);
                owner.native.borrow_mut().on_hover(move |hovered| {
                    callback(|| {
                        if let Some(owner) = weak.upgrade() {
                            owner.interaction.state(&owner, state::HOVERED, hovered)?;
                            owner.emit(
                                if hovered {
                                    EventKind::HoverIn
                                } else {
                                    EventKind::HoverOut
                                },
                                EventPayload::None,
                            );
                        }
                        Ok(())
                    })
                });
            } else {
                owner.native.borrow_mut().off_event(E::OnHover)?;
            }
        }
        let delay = self
            .props
            .borrow()
            .long_press_delay
            .unwrap_or(500.0)
            .clamp(1.0, i32::MAX as f32) as u32;
        let wants_long = !blocked
            && (owner.selectable()
                || events.contains(EventKind::LongPress)
                || owner.text_listens(EventKind::LongPress));
        if self.long_press.borrow().as_ref().map(|(delay, _)| *delay) != wants_long.then_some(delay)
        {
            self.clear_long(owner)?;
            if wants_long {
                let gesture =
                    Gesture::create_long_gesture(1, false, delay.min(i32::MAX as u32) as i32)?;
                let weak = Rc::downgrade(owner);
                gesture.on_gesture(GestureEventAction::Accept, move |event| {
                    if let Some(owner) = weak.upgrade() {
                        if owner.touches.borrow().selection_dragging(owner.id) {
                            return;
                        }
                        owner.interaction.long_fired.set(true);
                        let (x, y) = event.input.map_or((0.0, 0.0), |input| (input.x, input.y));
                        owner.press(EventKind::LongPress, x, y);
                        callback(|| {
                            owner.selection_begin(x, y)?;
                            Ok(())
                        });
                    }
                })?;
                owner
                    .native
                    .borrow()
                    .add_gesture_ref(&gesture, None, None)?;
                *self.long_press.borrow_mut() = Some((delay, gesture));
            }
        }
        Ok(())
    }
    pub(crate) fn props(&self, owner: &Rc<Node>, props: ViewProps) -> Result<()> {
        {
            let native = owner.native.borrow();
            if let Some(id) = &props.test_id {
                native.id(id.as_ref())?;
            } else {
                native.reset_attribute(A::Id)?;
            }
            native.set_bool_attribute(
                A::AccessibilityGroup,
                props.accessibility.accessible.unwrap_or(false),
            )?;
            let mode = if props.accessibility.hidden {
                AccessibilityMode::DisabledForDescendants
            } else {
                match props.accessibility.accessible {
                    Some(true) => AccessibilityMode::Enabled,
                    Some(false) => AccessibilityMode::Disabled,
                    None => AccessibilityMode::Auto,
                }
            };
            native.set_i32_attribute(A::AccessibilityMode, mode.into())?;
            native.set_string_attribute(
                A::AccessibilityDescription,
                props.accessibility.hint.as_deref().unwrap_or_default(),
            )?;
        }
        crate::accessibility::node::configure(
            owner,
            &self.props.borrow().accessibility,
            &props.accessibility,
        )?;
        let disabled = props.accessibility.state.disabled;
        *self.props.borrow_mut() = props;
        owner.update_wrapper()?;
        self.hit_region(owner)?;
        self.state(owner, state::DISABLED, disabled)?;
        self.apply_label(owner)?;
        self.apply_opacity(owner)?;
        self.pointer(owner, self.inherited_block.get())
    }
    pub(crate) fn label(&self, owner: &Node, label: Option<Arc<str>>) -> Result<()> {
        self.props.borrow_mut().accessibility.label = label;
        self.apply_label(owner)
    }
    pub(crate) fn text(&self, owner: &Node, text: &str) -> Result<()> {
        *self.label.borrow_mut() = text.to_owned();
        self.apply_label(owner)
    }
    fn apply_label(&self, owner: &Node) -> Result<()> {
        owner.native.borrow().set_string_attribute(
            A::AccessibilityText,
            self.props
                .borrow()
                .accessibility
                .label
                .as_deref()
                .unwrap_or(&self.label.borrow()),
        )?;
        Ok(())
    }
    pub(crate) fn pointer(&self, owner: &Rc<Node>, inherited: bool) -> Result<()> {
        self.inherited_block.set(inherited);
        let pointer = self.props.borrow().pointer_events;
        let mode = if inherited {
            HitTestMode::None
        } else {
            match pointer {
                PointerEvents::Auto => HitTestMode::Default,
                PointerEvents::BoxOnly => HitTestMode::Block,
                PointerEvents::BoxNone | PointerEvents::None => HitTestMode::None,
            }
        };
        owner
            .native
            .borrow()
            .set_i32_attribute(A::HitTestBehavior, mode.into())?;
        let internal =
            if inherited || matches!(pointer, PointerEvents::None | PointerEvents::BoxOnly) {
                HitTestMode::None
            } else {
                HitTestMode::Default
            };
        if let Some(input) = owner.input.borrow().as_ref() {
            input.hit_test(internal)?;
        }
        if let Some(scroll) = owner.scroll.borrow().as_ref() {
            scroll
                .native
                .borrow()
                .set_i32_attribute(A::HitTestBehavior, internal.into())?;
        }
        self.events(owner, self.events.get())?;
        for child in owner
            .children
            .borrow()
            .iter()
            .filter_map(|child| child.upgrade())
        {
            child.interaction.pointer(
                &child,
                inherited || matches!(pointer, PointerEvents::None | PointerEvents::BoxOnly),
            )?;
        }
        Ok(())
    }
    pub(crate) fn blocks_children(&self) -> bool {
        self.inherited_block.get()
            || matches!(
                self.props.borrow().pointer_events,
                PointerEvents::None | PointerEvents::BoxOnly
            )
    }
    pub(crate) fn gestures(&self, owner: &Rc<Node>, values: GestureValues) -> Result<()> {
        if self.gesture_values.replace(values) == values {
            return Ok(());
        }
        for gesture in self.gestures.borrow_mut().drain(..) {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        for kind in 0..3 {
            let enabled = match kind {
                0 => values.pan_x.is_some() || values.pan_y.is_some(),
                1 => values.pinch.is_some(),
                _ => values.rotation.is_some(),
            };
            if !enabled {
                continue;
            }
            let gesture = match kind {
                0 => Gesture::create_pan_gesture(
                    1,
                    GestureDirection::All,
                    10.0 * owner.scale as f64,
                )?,
                1 => Gesture::create_pinch_gesture(2, 5.0 * owner.scale as f64)?,
                _ => Gesture::create_rotation_gesture(2, 1.0)?,
            };
            let weak = Rc::downgrade(owner);
            gesture.on_gesture(
                GestureEventAction::Accept
                    | GestureEventAction::Update
                    | GestureEventAction::End
                    | GestureEventAction::Cancel,
                move |event| {
                    callback(|| {
                        let Some(owner) = weak.upgrade() else {
                            return Ok(());
                        };
                        let values = owner.interaction.gesture_values.get();
                        let accept = event.event_action_type == GestureEventAction::Accept;
                        let ended = event
                            .event_action_type
                            .intersects(GestureEventAction::End | GestureEventAction::Cancel);
                        if accept {
                            owner.interaction.gesture_start.set([
                                values.pan_x.map_or(0.0, |id| owner.animator.value(id)),
                                values.pan_y.map_or(0.0, |id| owner.animator.value(id)),
                                values.pinch.map_or(1.0, |id| owner.animator.value(id)),
                                values.rotation.map_or(0.0, |id| owner.animator.value(id)),
                            ]);
                        }
                        let start = owner.interaction.gesture_start.get();
                        match event.event_action_data {
                            GestureData::Pan(pan) => {
                                if let Some(id) = values.pan_x {
                                    owner
                                        .animator
                                        .set(id, start[0] + pan.offset_x / owner.scale)?;
                                }
                                if let Some(id) = values.pan_y {
                                    owner
                                        .animator
                                        .set(id, start[1] + pan.offset_y / owner.scale)?;
                                }
                                if accept || ended {
                                    owner.emit(
                                        if accept {
                                            EventKind::PanStart
                                        } else {
                                            EventKind::PanEnd
                                        },
                                        EventPayload::Pan {
                                            translation: nsh_core::Point::new(
                                                nsh_core::Points(pan.offset_x / owner.scale),
                                                nsh_core::Points(pan.offset_y / owner.scale),
                                            ),
                                            velocity_x: pan.velocity_x / owner.scale,
                                            velocity_y: pan.velocity_y / owner.scale,
                                            value_x: values.pan_x.map(|id| {
                                                nsh_core::Points(owner.animator.value(id))
                                            }),
                                            value_y: values.pan_y.map(|id| {
                                                nsh_core::Points(owner.animator.value(id))
                                            }),
                                        },
                                    );
                                }
                            }
                            GestureData::Pinch(pinch) => {
                                if let Some(id) = values.pinch {
                                    owner.animator.set(id, start[2] * pinch.scale)?;
                                }
                                if accept || ended {
                                    owner.emit(
                                        if accept {
                                            EventKind::PinchStart
                                        } else {
                                            EventKind::PinchEnd
                                        },
                                        EventPayload::Map(vec![
                                            ("scale", PayloadValue::Number(pinch.scale as f64)),
                                            ("velocity", PayloadValue::Number(0.0)),
                                        ]),
                                    );
                                }
                            }
                            GestureData::Rotation(rotation) => {
                                if let Some(id) = values.rotation {
                                    owner.animator.set(id, start[3] + rotation.angle)?;
                                }
                                if accept || ended {
                                    owner.emit(
                                        if accept {
                                            EventKind::RotationStart
                                        } else {
                                            EventKind::RotationEnd
                                        },
                                        EventPayload::Map(vec![
                                            (
                                                "rotation",
                                                PayloadValue::Number(rotation.angle as f64),
                                            ),
                                            ("velocity", PayloadValue::Number(0.0)),
                                        ]),
                                    );
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    })
                },
            )?;
            owner
                .native
                .borrow()
                .add_gesture_ref(&gesture, None, None)?;
            self.gestures.borrow_mut().push(gesture);
        }
        Ok(())
    }
    pub(crate) fn dispose(&self, owner: &Node) -> Result<()> {
        self.clear_long(owner)?;
        for gesture in self.gestures.borrow_mut().drain(..) {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        Ok(())
    }
    pub(crate) fn clear_long(&self, owner: &Node) -> Result<()> {
        if let Some((_, gesture)) = self.long_press.borrow_mut().take() {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        Ok(())
    }
    pub(crate) fn track(&self, owner: &Rc<Node>, tracked: u8) -> Result<()> {
        self.tracked.set(tracked);
        owner.control.set_states(owner.id, self.states.get());
        self.events(owner, self.events.get())
    }
    pub(crate) fn state(&self, owner: &Node, bit: u8, enabled: bool) -> Result<()> {
        let old = self.states.get();
        let next = if enabled { old | bit } else { old & !bit };
        self.states.set(next);
        if old != next && self.tracked.get() & bit != 0 {
            owner.control.set_states(owner.id, next);
        }
        if bit == state::PRESSED {
            self.apply_opacity(owner)?;
        }
        Ok(())
    }
    pub(crate) fn opacity(&self, owner: &Node, opacity: f32) -> Result<()> {
        self.opacity.set(Some(opacity));
        self.apply_opacity(owner)
    }
    fn apply_opacity(&self, owner: &Node) -> Result<()> {
        owner
            .visual_node()
            .borrow()
            .opacity(self.opacity.get().unwrap_or(1.0))?;
        Ok(())
    }
}
