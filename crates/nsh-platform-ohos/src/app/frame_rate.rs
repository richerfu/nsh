//! One window VSync vote, shared by the surface's rendering lanes. Ordinary
//! frames use the system's normal range; live shader animation raises its floor.
use std::sync::{Arc, Mutex};

use ohos_vsync_binding::{OH_NativeVSync_ExpectedRateRange, Vsync};

pub(crate) struct FrameRate {
    vsync: Arc<Vsync>,
    animations: Mutex<usize>,
}
impl FrameRate {
    pub(crate) fn new(vsync: Arc<Vsync>) -> Arc<Self> {
        Arc::new(Self {
            vsync,
            animations: Mutex::new(0),
        })
    }
    pub(crate) fn vsync(&self) -> &Vsync {
        &self.vsync
    }
    pub(crate) fn vote(self: &Arc<Self>) -> Vote {
        Vote {
            rate: self.clone(),
            active: false,
        }
    }
}
pub(crate) struct Vote {
    rate: Arc<FrameRate>,
    active: bool,
}
impl Vote {
    pub(crate) fn update(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        let mut count = self.rate.animations.lock().unwrap();
        let was_high = *count > 0;
        if active {
            *count += 1;
        } else {
            *count -= 1;
        }
        let high = *count > 0;
        if was_high != high {
            let status =
                self.rate
                    .vsync
                    .set_expected_frame_rate_range(OH_NativeVSync_ExpectedRateRange {
                        min: if high { 120 } else { 0 },
                        max: 120,
                        expected: if high { 120 } else { 60 },
                    });
            if status != 0 {
                log::warn!("OHOS VSync rate vote failed: {status}");
            }
        }
    }
}
impl Drop for Vote {
    fn drop(&mut self) {
        self.update(false);
    }
}
