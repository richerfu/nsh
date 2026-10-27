//! Native dialog presentation. Close animations retain the native
//! subtree until completion; the mounter then detaches and retires it.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::Instant,
};

use nsh_core::{
    EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig, ModalPresentation, Points,
};
use ohos_arkui_binding::{
    api::node_custom_event::NodeCustomEvent, dialog::Dialog, types::alignment::Alignment,
};

use crate::render::{
    geometry::Size,
    node::{Node, Result},
};
struct Transition {
    start: Instant,
    from: f32,
    to: f32,
    duration: f32,
}
pub(crate) struct Modal {
    owner: Weak<Node>,
    dialog: RefCell<Dialog>,
    config: Cell<ModalConfig>,
    shown: Cell<bool>,
    progress: Cell<f32>,
    transition: RefCell<Option<Transition>>,
    closing: Cell<bool>,
    insets: Cell<EdgeInsets>,
    reported: Cell<nsh_core::navigation::Reported>,
    overlay: RefCell<Option<crate::app::session::Overlay>>,
}
impl Modal {
    pub(crate) fn new(owner: Weak<Node>) -> Result<Rc<Self>> {
        let dialog = Dialog::new()?;
        dialog.custom_style(true)?;
        dialog.custom_animation(true)?;
        dialog.modal_mode(true)?;
        dialog.auto_cancel(true)?;
        dialog.content_alignment(Alignment::Center)?;
        let modal = Rc::new(Self {
            owner,
            dialog: RefCell::new(dialog),
            config: Cell::new(ModalConfig::default()),
            shown: Cell::new(false),
            progress: Cell::new(0.0),
            transition: RefCell::default(),
            closing: Cell::new(false),
            insets: Cell::new(EdgeInsets::default()),
            reported: Cell::new(nsh_core::navigation::Reported::default()),
            overlay: RefCell::default(),
        });
        let weak = Rc::downgrade(&modal);
        modal.dialog.borrow().dismiss_handler(move |reason| {
            if let Some(modal) = weak.upgrade() {
                crate::render::node::callback(|| modal.on_dismiss_attempt(reason.into()));
            }
            // Dismissal is always answered from mounted props and animated here.
            true
        })?;
        Ok(modal)
    }
    pub(crate) fn configure(&self, config: ModalConfig) -> Result<()> {
        self.config.set(config);
        self.dialog.borrow().content_alignment(
            if config.presentation == ModalPresentation::FullScreen {
                Alignment::Center
            } else {
                Alignment::Bottom
            },
        )?;
        let radius = if config.presentation == ModalPresentation::FullScreen {
            0.0
        } else {
            16.0
        };
        self.dialog
            .borrow()
            .corner_radius(radius, radius, 0.0, 0.0)?;
        self.theme()
    }
    fn theme(&self) -> Result<()> {
        let config = self.config.get();
        let full = config.presentation == ModalPresentation::FullScreen;
        // A full-screen modal owns its backdrop and hit-test surface.
        // ArkUI's default modal mask would dim that backdrop a second time.
        // Sheets still need the native mask outside their smaller content bounds.
        self.dialog.borrow().modal_mode(!full)?;
        let dark = self.owner.upgrade().is_some_and(|owner| {
            owner
                .appearance
                .get()
                .resolve(owner.platform.get().color_scheme)
                == nsh_engine::ColorScheme::Dark
        });
        self.dialog.borrow().background_color(if full {
            0
        } else if dark {
            0xff121212
        } else {
            0xffffffff
        })?;
        Ok(())
    }
    pub(crate) fn present(&self) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        if !self.shown.replace(true) {
            self.theme()?;
            self.dialog.borrow().content_node(&owner.mounted())?;
            self.dialog.borrow().show()?;
            *self.overlay.borrow_mut() = Some(owner.host.overlay(owner.dispatcher.clone())?);
        }
        self.closing.set(false);
        self.begin(1.0)?;
        Ok(())
    }
    pub(crate) fn request_close(&self) -> Result<()> {
        self.on_dismiss_attempt(0)
    }
    fn on_dismiss_attempt(&self, reason: i32) -> Result<()> {
        if self.closing.get() {
            return Ok(());
        }
        let config = self.config.get();
        let (reason_str, should_close) = match reason {
            1 => ("backdropPress", config.close_on_backdrop_press),
            3 => ("swipe", config.close_on_swipe),
            _ => ("cancel", config.close_on_escape),
        };
        if let Some(owner) = self.owner.upgrade() {
            owner.emit(
                EventKind::RequestClose,
                EventPayload::Map(vec![(
                    "reason",
                    nsh_core::PayloadValue::Str(reason_str.into()),
                )]),
            );
        }
        if should_close {
            self.close(true)?;
        }
        Ok(())
    }
    pub(crate) fn is_visible(&self) -> bool {
        self.shown.get()
    }
    pub(crate) fn close(&self, animated: bool) -> Result<()> {
        if !self.shown.get() {
            return Ok(());
        }
        if self.closing.replace(true) && animated {
            return Ok(());
        }
        if animated {
            self.begin(0.0)?;
        } else {
            self.finish(0.0)?;
        }
        Ok(())
    }
    fn begin(&self, to: f32) -> Result<()> {
        let reduced = self
            .owner
            .upgrade()
            .is_some_and(|owner| owner.platform.get().reduce_motion);
        let duration = if self.config.get().animation == ModalAnimation::None || reduced {
            0.0
        } else {
            nsh_core::navigation::MODAL.as_secs_f32() * (self.progress.get() - to).abs()
        };
        if duration <= 0.0 {
            return self.finish(to);
        }
        *self.transition.borrow_mut() = Some(Transition {
            start: Instant::now(),
            from: self.progress.get(),
            to,
            duration,
        });
        self.paint()?;
        if let Some(owner) = self.owner.upgrade() {
            owner.animator.request_frame();
        }
        Ok(())
    }
    pub(crate) fn frame(&self) -> Result<bool> {
        let state = self.transition.borrow().as_ref().map(|transition| {
            let t =
                (transition.start.elapsed().as_secs_f32() / transition.duration).clamp(0.0, 1.0);
            let eased = nsh_core::navigation::ease(t as f64);
            (
                transition.from + (transition.to - transition.from) * eased,
                t >= 1.0,
                transition.to,
            )
        });
        if let Some((value, done, to)) = state {
            self.progress.set(value);
            self.paint()?;
            if done {
                self.finish(to)?;
            }
        }
        Ok(self.transition.borrow().is_some())
    }
    fn paint(&self) -> Result<()> {
        if let Some(owner) = self.owner.upgrade() {
            let progress = self.progress.get();
            let fade = if self.config.get().animation == ModalAnimation::Fade {
                progress
            } else {
                1.0
            };
            let dy = if self.config.get().animation == ModalAnimation::Slide {
                nsh_core::navigation::modal_in_offset(Points(owner.frame.get().height), progress).0
            } else {
                0.0
            };
            owner.presentation(fade, dy)?;
        }
        Ok(())
    }
    fn finish(&self, to: f32) -> Result<()> {
        self.transition.borrow_mut().take();
        self.progress.set(to);
        if to == 0.0 {
            if self.shown.replace(false) {
                self.dialog.borrow().close()?;
                self.overlay.borrow_mut().take();
                self.dialog.borrow().remove_content()?;
                if let Some(owner) = self.owner.upgrade() {
                    owner.emit(EventKind::Dismiss, EventPayload::None);
                }
            }
        } else {
            self.paint()?;
            if let Some(owner) = self.owner.upgrade() {
                owner.emit(EventKind::Show, EventPayload::None);
            }
        }
        Ok(())
    }
    pub(crate) fn safe_top(&self) -> f32 {
        self.insets.get().top.0
    }
    pub(crate) fn closing(&self) -> bool {
        self.closing.get() && self.shown.get()
    }
    pub(crate) fn measure(&self, event: &NodeCustomEvent) -> Result<Size> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(Size::default());
        };
        let size = if let Some(constraint) = event.layout_constraint_in_measure() {
            let bound = |value: i32, fallback: f32| {
                let value = value as f32 / owner.scale;
                if value > 0.0 && value <= 16384.0 {
                    value
                } else {
                    fallback
                }
            };
            let width = bound(constraint.max_width(), owner.platform.get().window.width.0);
            let height = bound(
                constraint.max_height(),
                owner.platform.get().window.height.0,
            );
            match self.config.get().presentation {
                ModalPresentation::FullScreen => Size::new(width, height),
                ModalPresentation::PageSheet => Size::new(
                    width.min(720.0),
                    (height - owner.platform.get().safe_area.top.0 - 24.0).max(0.0),
                ),
                ModalPresentation::FormSheet => {
                    Size::new(width.min(540.0), (height - 32.0).clamp(0.0, 620.0))
                }
            }
        } else {
            Size::new(owner.frame.get().width, owner.frame.get().height)
        };
        let safe = owner.platform.get().safe_area;
        let full = self.config.get().presentation == ModalPresentation::FullScreen;
        let insets = EdgeInsets {
            top: if full { safe.top } else { Points::ZERO },
            bottom: safe.bottom,
            left: if size.width >= owner.platform.get().window.width.0 {
                safe.left
            } else {
                Points::ZERO
            },
            right: if size.width >= owner.platform.get().window.width.0 {
                safe.right
            } else {
                Points::ZERO
            },
        };
        self.insets.set(insets);
        if size.height > 0.0 {
            // Reported only on change, as every backend: the core's root size, and
            // `onInsets` (`safeAreaInsets()` inside the modal) when the insets changed.
            let mut reported = self.reported.get();
            let changed = reported.update(size.into(), insets);
            self.reported.set(reported);
            if let Some(insets_changed) = changed {
                owner.control.set_root_size(owner.id, size.into(), insets);
                if insets_changed {
                    owner.emit(
                        EventKind::Insets,
                        nsh_core::navigation::insets_payload(insets),
                    );
                }
            }
        }
        Ok(size)
    }
}
impl Drop for Modal {
    fn drop(&mut self) {
        let _ = self.dialog.borrow().close();
        let _ = self.dialog.borrow().remove_content();
    }
}
