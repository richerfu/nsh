//! Maps native VSync timestamps into the animator's elapsed-time domain. The
//! first frame anchors the domains; callback delivery latency cannot stretch
//! subsequent animation frames. Input between frames uses the elapsed clock.
#[derive(Default)]
pub(crate) struct FrameClock {
    anchor: Option<(u64, f64)>,
    last: f64,
}
impl FrameClock {
    pub(crate) fn sample(&mut self, timestamp: Option<u64>, elapsed: f64) -> f64 {
        let now = if let Some(timestamp) = timestamp.filter(|stamp| *stamp > 0) {
            let (origin, start) = *self.anchor.get_or_insert((timestamp, elapsed));
            start + timestamp.saturating_sub(origin) as f64 / 1_000_000.0
        } else {
            elapsed
        };
        self.last = now.max(self.last);
        self.last
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_delivery_uses_the_frame_time_and_never_rewinds() {
        let mut clock = FrameClock::default();
        assert_eq!(clock.sample(Some(1_000_000_000), 50.0), 50.0);
        assert_eq!(clock.sample(Some(1_008_000_000), 90.0), 58.0);
        assert_eq!(clock.sample(Some(1_016_000_000), 91.0), 66.0);
        assert_eq!(clock.sample(Some(1_008_000_000), 92.0), 66.0);
    }
}
