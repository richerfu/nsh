//! UI-thread layout/style transitions and keyframes, sharing the desktop
//! backend's interpolation semantics and the surface's native frame clock.
use std::sync::Arc;

use nsh_core::{
    BoxShadow, Color, Corners, Edges, KeyframeAnimation, Keyframes, LayoutAnimation, LayoutCurve,
    LayoutProperty, LayoutTransition, Points, Transform, TransitionProps, ViewStyle,
};

use crate::render::geometry::Rect;

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Extra {
    pub(crate) rotate: f32,
    pub(crate) scale: f32,
    pub(crate) shift_y: f32,
}
impl Extra {
    pub(crate) const NONE: Self = Self {
        rotate: 0.0,
        scale: 1.0,
        shift_y: 0.0,
    };
}
#[derive(Default)]
pub(crate) struct Live {
    frame: Option<FrameTrack>,
    style: Option<StyleTrack>,
    presence: Option<Presence>,
    keyframes: Option<KeyframeRun>,
}
pub(crate) struct Step {
    pub(crate) frame: Option<Rect>,
    pub(crate) opacity: f32,
    pub(crate) extra: Extra,
    pub(crate) active: bool,
    pub(crate) removed: bool,
}
impl Live {
    pub(crate) fn finish(&mut self, style: &mut ViewStyle) -> Step {
        let frame = self.frame.take().map(|track| track.to);
        if let Some(track) = self.style.take() {
            *style = *track.to;
        }
        let removed = self.presence.take().is_some_and(|p| p.to == 0.0);
        self.keyframes = None;
        Step {
            frame,
            opacity: 1.0,
            extra: Extra::NONE,
            active: false,
            removed,
        }
    }
    pub(crate) fn frame_target(&self) -> Option<Rect> {
        self.frame.as_ref().map(|t| t.to)
    }
    pub(crate) fn frame(
        &mut self,
        from: Rect,
        to: Rect,
        animation: Option<&LayoutAnimation>,
    ) -> bool {
        self.frame = animation
            .and_then(|a| a.update)
            .filter(|_| from != Rect::default() && from != to)
            .map(|t| FrameTrack {
                from,
                to,
                timing: Timing::layout(&t),
            });
        self.frame.is_some()
    }
    pub(crate) fn style(
        &mut self,
        out: &mut ViewStyle,
        old: ViewStyle,
        reduce: bool,
        initialized: bool,
    ) {
        let animation = out.animation;
        let changed = animation != old.animation;
        self.style = out
            .transition
            .filter(|t| {
                initialized
                    && !reduce
                    && t.duration + t.delay > 0.0
                    && differs(&old, out, t.properties)
            })
            .map(|t| {
                let target = out.clone();
                lerp_style(out, &old, &target, t.properties, 0.0);
                StyleTrack {
                    from: Box::new(old),
                    to: Box::new(target),
                    props: t.properties,
                    timing: Timing::new(t.delay, t.duration, Curve::Bezier(t.easing)),
                }
            });
        if changed || reduce {
            self.keyframes = animation.filter(|_| !reduce).map(|animation| KeyframeRun {
                animation,
                start: None,
            });
        }
    }
    pub(crate) fn present(&mut self, transition: LayoutTransition, from: f32, to: f32) {
        self.presence = Some(Presence {
            property: transition.property,
            from,
            to,
            value: from,
            timing: Timing::layout(&transition),
        });
    }
    pub(crate) fn cancel_removal(&mut self) {
        self.presence = None;
    }
    pub(crate) fn active(&self) -> bool {
        self.frame.is_some()
            || self.style.is_some()
            || self.presence.is_some()
            || self.keyframes.is_some()
    }
    pub(crate) fn step(&mut self, now: f64, style: &mut ViewStyle) -> Step {
        let mut step = Step {
            frame: None,
            opacity: 1.0,
            extra: Extra::NONE,
            active: false,
            removed: false,
        };
        if let Some(track) = &mut self.frame
            && let Some((p, done)) = track.timing.at(now)
        {
            step.frame = Some(if done {
                track.to
            } else {
                lerp_rect(track.from, track.to, p)
            });
            if done {
                self.frame = None;
            }
        }
        if let Some(track) = &mut self.style
            && let Some((p, done)) = track.timing.at(now)
        {
            lerp_style(
                style,
                &track.from,
                &track.to,
                track.props,
                if done { 1.0 } else { p },
            );
            if done {
                self.style = None;
            }
        }
        if let Some(presence) = &mut self.presence {
            let result = presence.timing.at(now);
            if let Some((p, done)) = result {
                presence.value = if done {
                    presence.to
                } else {
                    lerp(presence.from, presence.to, p)
                };
            }
            step.opacity = match presence.property {
                LayoutProperty::Opacity => presence.value.clamp(0.0, 1.0),
                LayoutProperty::Scale => 1.0,
            };
            if presence.property == LayoutProperty::Scale {
                step.extra.scale = presence.value.max(0.01);
            }
            if result.is_some_and(|(_, done)| done) {
                step.removed = presence.to == 0.0;
                self.presence = None;
            }
        }
        if let Some(run) = &mut self.keyframes {
            let start = *run.start.get_or_insert(now);
            if let Some((opacity, extra)) = keyframes_at(&run.animation, (now - start) as f32) {
                step.opacity *= opacity;
                step.extra.rotate += extra.rotate;
                step.extra.scale *= extra.scale;
                step.extra.shift_y += extra.shift_y;
            } else {
                self.keyframes = None;
            }
        }
        step.active = self.active();
        step
    }
}

/// An easing curve, from progress in time to progress in value.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Curve {
    Linear,
    /// Cubic Bézier `x1, y1, x2, y2`.
    Bezier([f32; 4]),
    /// A spring with this damping ratio that settles within the duration.
    Spring(f32),
}

impl Curve {
    fn of(curve: LayoutCurve) -> Curve {
        match curve {
            LayoutCurve::Linear => Curve::Linear,
            // UIKit's curves.
            LayoutCurve::EaseIn => Curve::Bezier([0.42, 0.0, 1.0, 1.0]),
            LayoutCurve::EaseOut => Curve::Bezier([0.0, 0.0, 0.58, 1.0]),
            LayoutCurve::EaseInEaseOut | LayoutCurve::Keyboard => {
                Curve::Bezier([0.42, 0.0, 0.58, 1.0])
            }
            LayoutCurve::Spring { damping } => Curve::Spring(damping),
        }
    }

    fn at(self, x: f32) -> f32 {
        match self {
            Curve::Linear => x,
            Curve::Bezier(b) => cubic_bezier(b, x),
            Curve::Spring(damping) => spring(damping, x),
        }
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `x` in `0..=1`.
fn cubic_bezier([x1, y1, x2, y2]: [f32; 4], x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // Polynomial coefficients of B(t) = ((a t + b) t + c) t.
    let (cx, cy) = (3.0 * x1, 3.0 * y1);
    let (bx, by) = (3.0 * (x2 - x1) - cx, 3.0 * (y2 - y1) - cy);
    let (ax, ay) = (1.0 - cx - bx, 1.0 - cy - by);
    let sample_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let slope_x = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;
    // Newton's method converges in a few steps on most curves; bisection
    // catches the flat ones.
    let mut t = x;
    let mut solved = false;
    for _ in 0..8 {
        let err = sample_x(t) - x;
        if err.abs() < 1e-6 {
            solved = true;
            break;
        }
        let d = slope_x(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    if !solved || !(0.0..=1.0).contains(&t) {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..32 {
            let v = sample_x(t);
            if (v - x).abs() < 1e-6 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
    }
    ((ay * t + by) * t + cy) * t
}

/// A damped spring from 0 to 1 at `x` (the fraction of the duration), its
/// stiffness chosen so that it settles by the end, as UIKit's
/// `usingSpringWithDamping` fits a spring into the given duration.
fn spring(damping: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // The envelope falls to 1/1000 by the end.
    const SETTLE: f32 = 6.907_755; // ln(1000)
    let zeta = damping.clamp(0.05, 1.0);
    if zeta > 0.999 {
        // Critically damped: (1 + w x) e^(-w x) reaches 1/1000 at w = 9.23.
        let w = 9.233;
        return 1.0 - (1.0 + w * x) * (-w * x).exp();
    }
    let w = SETTLE / zeta;
    let wd = w * (1.0 - zeta * zeta).sqrt();
    let envelope = (-zeta * w * x).exp();
    1.0 - envelope * ((wd * x).cos() + zeta * w / wd * (wd * x).sin())
}

/// When a change runs and how it eases.
#[derive(Debug, Clone, Copy)]
struct Timing {
    /// Frame time (ms) of the first tick.
    start: Option<f64>,
    delay: f32,
    duration: f32,
    curve: Curve,
}

impl Timing {
    fn new(delay: f32, duration: f32, curve: Curve) -> Self {
        Timing {
            start: None,
            delay: delay.max(0.0),
            duration: duration.max(0.0),
            curve,
        }
    }

    fn layout(t: &LayoutTransition) -> Self {
        Timing::new(t.delay_ms, t.duration_ms, Curve::of(t.curve))
    }

    /// Eased progress at `now` (ms): `None` while delayed, then the value
    /// and whether it finished.
    fn at(&mut self, now: f64) -> Option<(f32, bool)> {
        let start = *self.start.get_or_insert(now);
        progress(self.curve, self.delay, self.duration, (now - start) as f32)
    }
}

/// Eased progress `elapsed` ms after a change began.
fn progress(curve: Curve, delay: f32, duration: f32, elapsed: f32) -> Option<(f32, bool)> {
    let t = elapsed - delay;
    if t < 0.0 {
        return None;
    }
    if duration <= 0.0 || t >= duration {
        return Some((1.0, true));
    }
    Some((curve.at(t / duration), false))
}

struct FrameTrack {
    from: Rect,
    to: Rect,
    timing: Timing,
}

struct StyleTrack {
    from: Box<ViewStyle>,
    to: Box<ViewStyle>,
    props: TransitionProps,
    timing: Timing,
}

struct Presence {
    property: LayoutProperty,
    /// Visibility at the start and end (0 hidden, 1 shown).
    from: f32,
    to: f32,
    /// The current visibility.
    value: f32,
    timing: Timing,
}

struct KeyframeRun {
    animation: KeyframeAnimation,
    start: Option<f64>,
}

/// The opacity multiplier and extra transform of `animation`, `elapsed` ms
/// after it was set; `None` once a finite animation ended. During the delay
/// the view shows its own values, as CSS without `animation-fill-mode`.
fn keyframes_at(animation: &KeyframeAnimation, elapsed: f32) -> Option<(f32, Extra)> {
    let t = elapsed - animation.delay;
    if t < 0.0 {
        return Some((1.0, Extra::NONE));
    }
    let d = animation.duration;
    if d <= 0.0 || animation.iterations.is_some_and(|n| t >= n as f32 * d) {
        return None;
    }
    let frac = (t % d) / d;
    let ease = |x: f32| cubic_bezier(animation.easing, x);
    let mut extra = Extra::NONE;
    let mut opacity = 1.0;
    match animation.keyframes {
        Keyframes::Spin => extra.rotate = 360.0 * ease(frac),
        Keyframes::Ping => {
            // 75%, 100% { transform: scale(2); opacity: 0 }
            let q = if frac < 0.75 { ease(frac / 0.75) } else { 1.0 };
            extra.scale = 1.0 + q;
            opacity = 1.0 - q;
        }
        Keyframes::Pulse => {
            // 50% { opacity: .5 }
            let q = if frac < 0.5 {
                ease(frac * 2.0)
            } else {
                1.0 - ease(frac * 2.0 - 1.0)
            };
            opacity = 1.0 - 0.5 * q;
        }
        Keyframes::Bounce => {
            // 0%, 100% { translateY(-25%) } eased in, 50% { none } eased
            // out; the animation's own easing does not apply.
            extra.shift_y = if frac < 0.5 {
                -0.25 * (1.0 - cubic_bezier([0.8, 0.0, 1.0, 1.0], frac * 2.0))
            } else {
                -0.25 * cubic_bezier([0.0, 0.0, 0.2, 1.0], frac * 2.0 - 1.0)
            };
        }
    }
    Some((opacity, extra))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_points(a: Points, b: Points, t: f32) -> Points {
    Points(lerp(a.0, b.0, t))
}

/// Interpolates colors in premultiplied RGBA, as CSS does, so that
/// transparent ends do not pass through black.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    if a == b {
        return a;
    }
    let t = t.clamp(0.0, 1.0);
    let parts = |c: Color| {
        let (r, g, b, a) = c.to_f64();
        let a = a as f32;
        [r as f32 * a, g as f32 * a, b as f32 * a, a]
    };
    let (pa, pb) = (parts(a), parts(b));
    let p: [f32; 4] = std::array::from_fn(|i| lerp(pa[i], pb[i], t));
    let alpha = p[3];
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::rgba(
        channel(p[0] / alpha),
        channel(p[1] / alpha),
        channel(p[2] / alpha),
        channel(alpha),
    )
}

/// Interpolates transform lists entry by entry when their kinds match (an
/// empty list stands for the identity of the other); `None` when they
/// cannot be interpolated and the new list applies at once.
fn lerp_transform(a: &Transform, b: &Transform, t: f32) -> Option<Transform> {
    let (ao, bo) = (a.ops(), b.ops());
    let same_kinds = ao.iter().map(|o| o.kind).eq(bo.iter().map(|o| o.kind));
    if !(ao.is_empty() || bo.is_empty() || same_kinds) {
        return None;
    }
    let kinds = if ao.is_empty() { bo } else { ao };
    let mut out = Transform::IDENTITY;
    for (i, op) in kinds.iter().enumerate() {
        let va = ao.get(i).map_or(op.kind.identity(), |o| o.value);
        let vb = bo.get(i).map_or(op.kind.identity(), |o| o.value);
        out.push(op.kind, lerp(va, vb, t));
    }
    Some(out)
}

/// A shadow that draws nothing, to interpolate against `s`.
fn none_like(s: &BoxShadow) -> BoxShadow {
    BoxShadow {
        offset_x: Points::ZERO,
        offset_y: Points::ZERO,
        blur: Points::ZERO,
        spread: Points::ZERO,
        color: Color::TRANSPARENT,
        inset: s.inset,
    }
}

/// Interpolates box shadow lists, the shorter padded with empty shadows
/// (CSS); `None` when an inset and an outer shadow meet.
fn lerp_box_shadows(a: &[BoxShadow], b: &[BoxShadow], t: f32) -> Option<Vec<BoxShadow>> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (sa, sb) = match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => (*x, *y),
            (Some(x), None) => (*x, none_like(x)),
            (None, Some(y)) => (none_like(y), *y),
            (None, None) => unreachable!("i < the longer length"),
        };
        if sa.inset != sb.inset {
            return None;
        }
        let l = |a: Points, b: Points| Points(lerp(a.0, b.0, t));
        out.push(BoxShadow {
            offset_x: l(sa.offset_x, sb.offset_x),
            offset_y: l(sa.offset_y, sb.offset_y),
            blur: l(sa.blur, sb.blur).max(Points::ZERO),
            spread: l(sa.spread, sb.spread),
            color: lerp_color(sa.color, sb.color, t),
            inset: sa.inset,
        });
    }
    Some(out)
}

fn box_shadows(s: &ViewStyle) -> &[BoxShadow] {
    s.effects.as_deref().map_or(&[], |e| &e.box_shadows)
}

/// True if `a` and `b` differ in what `props` animates.
fn differs(a: &ViewStyle, b: &ViewStyle, props: TransitionProps) -> bool {
    (props.has(TransitionProps::COLORS)
        && (a.background_color != b.background_color
            || a.borders.colors != b.borders.colors
            || a.outline.color != b.outline.color
            || a.tint_color != b.tint_color))
        || (props.has(TransitionProps::OPACITY) && a.opacity != b.opacity)
        || (props.has(TransitionProps::TRANSFORM) && a.transform != b.transform)
        || (props.has(TransitionProps::SHADOW)
            && (a.shadow != b.shadow || box_shadows(a) != box_shadows(b)))
        || (props.has(TransitionProps::SHAPE)
            && (a.border_radius != b.border_radius
                || a.corner_radius != b.corner_radius
                || a.borders.widths != b.borders.widths
                || a.outline.width != b.outline.width))
}

/// Writes the values between `from` and `to` that `props` animates into
/// `out` (a copy of `to`).
fn lerp_style(
    out: &mut ViewStyle,
    from: &ViewStyle,
    to: &ViewStyle,
    props: TransitionProps,
    t: f32,
) {
    if props.has(TransitionProps::COLORS) {
        out.background_color = lerp_color(from.background_color, to.background_color, t);
        out.borders.colors =
            std::array::from_fn(|i| lerp_color(from.borders.colors[i], to.borders.colors[i], t));
        out.outline.color = lerp_color(from.outline.color, to.outline.color, t);
        if let (Some(a), Some(b)) = (from.tint_color, to.tint_color) {
            out.tint_color = Some(lerp_color(a, b, t));
        }
    }
    if props.has(TransitionProps::OPACITY) {
        out.opacity = lerp(from.opacity, to.opacity, t).clamp(0.0, 1.0);
    }
    if props.has(TransitionProps::TRANSFORM)
        && let Some(transform) = lerp_transform(&from.transform, &to.transform, t)
    {
        out.transform = transform;
    }
    if props.has(TransitionProps::SHADOW) {
        let (a, b) = (&from.shadow, &to.shadow);
        out.shadow.color = lerp_color(a.color, b.color, t);
        out.shadow.offset_x = lerp_points(a.offset_x, b.offset_x, t);
        out.shadow.offset_y = lerp_points(a.offset_y, b.offset_y, t);
        out.shadow.opacity = lerp(a.opacity, b.opacity, t).clamp(0.0, 1.0);
        out.shadow.radius = lerp_points(a.radius, b.radius, t).max(Points::ZERO);
        let (sa, sb) = (box_shadows(from), box_shadows(to));
        if sa != sb
            && let Some(shadows) = lerp_box_shadows(sa, sb, t)
        {
            let mut effects = to.effects.as_deref().cloned().unwrap_or_default();
            effects.box_shadows = shadows;
            out.effects = Some(Arc::new(effects));
        }
    }
    if props.has(TransitionProps::SHAPE) {
        out.border_radius = lerp_points(from.border_radius, to.border_radius, t).max(Points::ZERO);
        let (ca, cb) = (from.corner_radius.to_array(), to.corner_radius.to_array());
        out.corner_radius = Corners::from(std::array::from_fn(|i| match (ca[i], cb[i]) {
            (None, None) => None,
            (a, b) => Some(
                lerp_points(
                    a.unwrap_or(from.border_radius),
                    b.unwrap_or(to.border_radius),
                    t,
                )
                .max(Points::ZERO),
            ),
        }));
        let (wa, wb) = (from.borders.widths.to_array(), to.borders.widths.to_array());
        out.borders.widths = Edges::from(std::array::from_fn(|i| {
            lerp_points(wa[i], wb[i], t).max(Points::ZERO)
        }));
        out.outline.width = Points(lerp(from.outline.width.0, to.outline.width.0, t).max(0.0));
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        width: lerp(a.width, b.width, t).max(0.0),
        height: lerp(a.height, b.height, t).max(0.0),
    }
}
