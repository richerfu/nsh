//! Coalesced live-region updates and the desired accessibility snapshot.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{Accessibility, LiveRegion};

use crate::render::node::{Node, Result};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) spec: RefCell<Accessibility>,
    text: RefCell<String>,
    dirty: Cell<bool>,
    announced: RefCell<Option<String>>,
    #[cfg(feature = "api-23")]
    hidden_by_provider: Cell<bool>,
}
impl State {
    pub(crate) fn configure(&self, owner: &Rc<Node>, spec: &Accessibility) {
        if *self.spec.borrow() != *spec {
            *self.spec.borrow_mut() = spec.clone();
            self.changed(owner);
        }
    }
    pub(crate) fn text(&self, owner: &Rc<Node>, text: &str) {
        if *self.text.borrow() != text {
            *self.text.borrow_mut() = text.to_owned();
            self.changed(owner);
        }
    }
    pub(crate) fn changed(&self, owner: &Rc<Node>) {
        if !self.dirty.replace(true) {
            owner
                .accessibility_dirty
                .borrow_mut()
                .push(Rc::downgrade(owner));
        }
        let mut parent = owner.parent_node.borrow().upgrade();
        while let Some(node) = parent {
            let state = &node.accessibility_state;
            if state.spec.borrow().live_region != LiveRegion::None && !state.dirty.replace(true) {
                owner
                    .accessibility_dirty
                    .borrow_mut()
                    .push(Rc::downgrade(&node));
            }
            parent = node.parent_node.borrow().upgrade();
        }
    }
    pub(crate) fn label(&self, owner: &Node) -> String {
        if let Some(label) = &self.spec.borrow().label {
            return label.to_string();
        }
        if !self.text.borrow().is_empty() {
            return self.text.borrow().clone();
        }
        owner
            .children
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|child| !child.accessibility_state.spec.borrow().hidden)
            .map(|child| child.accessibility_state.label(&child))
            .filter(|label| !label.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub(crate) fn flush(&self, owner: &Rc<Node>) -> Result<()> {
        self.dirty.set(false);
        owner.update_wrapper()?;
        if let Some(input) = owner.input.borrow().as_ref() {
            input.update();
            if !owner.wrapper_owns_semantics() {
                use ohos_arkui_binding::{
                    component::attribute::ArkUICommonAttribute,
                    types::{attribute::ArkUINodeAttributeType as A, view::AccessibilityMode},
                };
                // The editor owns selection, edit actions and its spoken value.
                // A grouped attachment container would consume directional focus.
                let native = owner.native.borrow();
                native.set_bool_attribute(A::AccessibilityGroup, false)?;
                native.set_i32_attribute(
                    A::AccessibilityMode,
                    if self.spec.borrow().hidden {
                        AccessibilityMode::DisabledForDescendants
                    } else {
                        AccessibilityMode::Disabled
                    }
                    .into(),
                )?;
            }
        }
        #[cfg(feature = "api-23")]
        owner.accessibility_provider.configure(owner)?;
        #[cfg(feature = "api-23")]
        self.provider_visibility(owner)?;
        let spec = self.spec.borrow();
        if spec.live_region == LiveRegion::None || spec.hidden || !owner.visible_in_tree() {
            self.announced.borrow_mut().take();
            return Ok(());
        }
        let mut text = self.label(owner);
        if let Some(value) = crate::accessibility::value::spoken_value(&spec) {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(&value);
        }
        let previous = self.announced.borrow_mut().replace(text.clone());
        if previous.as_ref().is_some_and(|old| *old != text) && !text.is_empty() {
            owner.host.accessibility_live(
                owner.dispatcher.clone(),
                text,
                spec.live_region == LiveRegion::Assertive,
            )?;
        }
        Ok(())
    }
    #[cfg(feature = "api-23")]
    fn provider_visibility(&self, owner: &Rc<Node>) -> Result<()> {
        use ohos_arkui_binding::{
            component::attribute::ArkUICommonAttribute,
            types::{attribute::ArkUINodeAttributeType as A, view::AccessibilityMode as M},
        };
        let mut parent = owner.parent_node.borrow().upgrade();
        let mut hidden = false;
        while let Some(node) = parent {
            if node.accessibility_provider.hides_native_children() {
                hidden = true;
                break;
            }
            parent = node.parent_node.borrow().upgrade();
        }
        if hidden {
            self.hidden_by_provider.set(true);
            owner
                .mounted()
                .borrow()
                .set_i32_attribute(A::AccessibilityMode, M::DisabledForDescendants.into())?;
        } else if self.hidden_by_provider.replace(false) {
            if owner.wrapper.borrow().is_some() {
                owner
                    .mounted()
                    .borrow()
                    .reset_attribute(A::AccessibilityMode)?;
            } else {
                let spec = self.spec.borrow();
                let mode = if spec.hidden {
                    M::DisabledForDescendants
                } else if owner.input.borrow().is_some() {
                    M::Disabled
                } else if owner.accessibility_provider.hides_native_children() {
                    M::Enabled
                } else {
                    match spec.accessible {
                        Some(true) => M::Enabled,
                        Some(false) => M::Disabled,
                        None => M::Auto,
                    }
                };
                owner
                    .native
                    .borrow()
                    .set_i32_attribute(A::AccessibilityMode, mode.into())?;
            }
        }
        Ok(())
    }
}
