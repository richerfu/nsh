//! Cached background paints and outlines; geometry comes from the shared core.
use std::{cell::RefCell, sync::Arc};

use nsh_core::{BorderStyle, Gradient, GradientKind, GradientSize, Size, ViewStyle};
use ohos_arkui_binding::{
    component::attribute::ArkUICommonAttribute,
    types::{attribute::ArkUINodeAttributeType as A, view::BlendMode as B},
};
use ohos_drawing_binding::{
    Brush, Canvas, ClipOperation, Filter, MaskFilter, Matrix, Path, PathEffect, PathFillType, Pen,
    ShaderEffect, TileMode,
};

use crate::{
    render::node::{Node, Result},
    text::typography::argb,
};
#[derive(Default)]
pub(crate) struct Decoration {
    backgrounds: RefCell<Option<Backgrounds>>,
    native: RefCell<Option<NativeEffects>>,
    shadows: RefCell<Option<Shadows>>,
}
struct Shadows {
    effects: Option<Arc<nsh_core::Effects>>,
    size: Size,
    radii: [f32; 4],
    borders: [f32; 4],
    continuous: bool,
    border: Path,
    padding: Path,
    paints: Vec<ShadowPaint>,
}
struct ShadowPaint {
    inset: bool,
    path: Path,
    brush: Brush,
}
struct Backgrounds {
    effects: Option<Arc<nsh_core::Effects>>,
    size: Size,
    paints: Vec<Brush>,
}
#[derive(Clone, PartialEq)]
struct NativeEffects {
    backdrop_blur: f32,
    blend: nsh_core::BlendMode,
}
impl Default for NativeEffects {
    fn default() -> Self {
        Self {
            backdrop_blur: 0.0,
            blend: nsh_core::BlendMode::Normal,
        }
    }
}
impl Decoration {
    pub(crate) fn native(&self, owner: &Node, style: &ViewStyle) -> Result<()> {
        use nsh_core::{BlendMode as M, Filter as F};
        for g in style
            .effects
            .as_ref()
            .into_iter()
            .flat_map(|e| &e.background_images)
        {
            if matches!(g.kind, GradientKind::Conic { .. }) && g.repeating && !g.stops.is_empty() {
                let stops = g.resolved_stops();
                let span = stops.last().unwrap().1 - stops[0].1;
                if span > 1e-5 && (1.0 / span + 3.0) * stops.len() as f32 > 4096.0 {
                    log::warn!("OHOS repeating conic gradient: limiting repeated stops to 4096");
                }
            }
        }
        let mut effect = NativeEffects {
            blend: if owner.scene_active() {
                nsh_core::BlendMode::Normal
            } else {
                style.blend_mode
            },
            ..NativeEffects::default()
        };
        for filter in style
            .effects
            .as_ref()
            .filter(|_| !owner.scene_active())
            .into_iter()
            .flat_map(|e| &e.backdrop_filters)
        {
            if let F::Blur(radius) = filter {
                effect.backdrop_blur += radius;
            }
        }
        let mut previous = self.native.borrow_mut();
        let default = NativeEffects::default();
        let old = previous.as_ref().unwrap_or(&default);
        let native = owner.native.borrow();
        if old.backdrop_blur != effect.backdrop_blur {
            native.set_f32_attribute(A::BackdropBlur, effect.backdrop_blur * owner.scale)?;
        }
        if old.blend != effect.blend {
            let mode = match effect.blend {
                M::Normal => B::None,
                M::Multiply => B::Multiply,
                M::Screen => B::Screen,
                M::Overlay => B::Overlay,
                M::Darken => B::Darken,
                M::Lighten => B::Lighten,
                M::ColorDodge => B::ColorDodge,
                M::ColorBurn => B::ColorBurn,
                M::HardLight => B::HardLight,
                M::SoftLight => B::SoftLight,
                M::Difference => B::Difference,
                M::Exclusion => B::Exclusion,
                M::Hue => B::Hue,
                M::Saturation => B::Saturation,
                M::Color => B::Color,
                M::Luminosity => B::Luminosity,
                M::PlusLighter => B::Plus,
                // Scene composition owns this mode. Hidden/detached triggers
                // can keep the style while their native exports retire.
                M::PlusDarker => B::None,
            };
            owner
                .visual_node()
                .borrow()
                .set_attribute(A::BlendMode, vec![i32::from(mode), 1].into())?;
        }
        *previous = Some(effect);
        Ok(())
    }
    pub(crate) fn background(&self, canvas: &Canvas, style: &ViewStyle, size: Size, scale: f32) {
        self.shadows(canvas, style, size, scale, false);
        let images = style
            .effects
            .as_ref()
            .map_or(&[][..], |e| e.background_images.as_slice());
        if images.is_empty() {
            self.backgrounds.borrow_mut().take();
            self.shadows(canvas, style, size, scale, true);
            return;
        }
        let mut backgrounds = self.backgrounds.borrow_mut();
        if backgrounds.as_ref().is_none_or(|c| {
            c.size != size
                || match (&c.effects, &style.effects) {
                    (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                    (None, None) => false,
                    _ => true,
                }
        }) {
            *backgrounds = Some(Backgrounds {
                effects: style.effects.clone(),
                size,
                paints: images.iter().rev().map(|g| gradient(g, size)).collect(),
            });
        }
        canvas.save();
        canvas.scale(scale, scale);
        let path = outline(style, size, 0.0);
        canvas.clip_path(&path, ClipOperation::Intersect, true);
        for brush in &backgrounds.as_ref().unwrap().paints {
            canvas.attach_brush(brush);
            canvas.draw_path(&path);
            canvas.detach_brush();
        }
        canvas.restore();
        self.shadows(canvas, style, size, scale, true);
    }
    fn shadows(&self, canvas: &Canvas, style: &ViewStyle, size: Size, scale: f32, inset: bool) {
        let layers = style
            .effects
            .as_ref()
            .map_or(&[][..], |effects| effects.box_shadows.as_slice());
        if layers.is_empty() {
            self.shadows.borrow_mut().take();
            return;
        }
        let radii = style
            .corner_radius
            .map(|radius| radius.unwrap_or(style.border_radius));
        let mut cache = self.shadows.borrow_mut();
        if cache.as_ref().is_none_or(|cache| {
            cache.size != size
                || cache.radii != radii
                || cache.borders != style.borders.widths
                || cache.continuous != style.continuous_corners
                || !match (&cache.effects, &style.effects) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }) {
            let padding = padding_outline(style, size, 0.0);
            let paints = layers
                .iter()
                .rev()
                .filter(|layer| layer.is_visible())
                .map(|layer| {
                    let mut brush = Brush::new();
                    brush.set_anti_alias(true);
                    brush.set_color(argb(layer.color));
                    let mask = MaskFilter::blur(layer.blur.max(0.0) * 0.5, true);
                    let mut filter = Filter::new();
                    filter.set_mask_filter(mask.as_ref());
                    brush.set_filter(Some(&filter));
                    let mut path = if layer.inset {
                        padding_outline(style, size, layer.spread)
                    } else {
                        outline(style, size, layer.spread)
                    };
                    path.transform(&Matrix::from_affine(
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                        layer.offset_x,
                        layer.offset_y,
                    ));
                    if layer.inset {
                        path.set_fill_type(PathFillType::InverseWinding);
                    }
                    ShadowPaint {
                        inset: layer.inset,
                        path,
                        brush,
                    }
                })
                .collect();
            *cache = Some(Shadows {
                effects: style.effects.clone(),
                size,
                radii,
                borders: style.borders.widths,
                continuous: style.continuous_corners,
                border: outline(style, size, 0.0),
                padding,
                paints,
            });
        }
        let cache = cache.as_ref().unwrap();
        canvas.save();
        canvas.scale(scale, scale);
        canvas.clip_path(
            if inset { &cache.padding } else { &cache.border },
            if inset {
                ClipOperation::Intersect
            } else {
                ClipOperation::Difference
            },
            true,
        );
        for paint in cache.paints.iter().filter(|paint| paint.inset == inset) {
            canvas.attach_brush(&paint.brush);
            canvas.draw_path(&paint.path);
            canvas.detach_brush();
        }
        canvas.restore();
    }
    pub(crate) fn outline(&self, canvas: &Canvas, style: &ViewStyle, size: Size, scale: f32) {
        let o = style.outline;
        if o.width <= 0.0 || o.color.is_transparent() {
            return;
        }
        let mut pen = Pen::new();
        pen.set_anti_alias(true);
        pen.set_color(argb(o.color));
        pen.set_width(o.width);
        let dash = match o.style {
            BorderStyle::Solid => None,
            BorderStyle::Dashed => PathEffect::dash(&[o.width * 3.0, o.width * 3.0], 0.0),
            BorderStyle::Dotted => PathEffect::dash(&[o.width, o.width], 0.0),
        };
        pen.set_path_effect(dash.as_ref());
        canvas.save();
        canvas.scale(scale, scale);
        canvas.attach_pen(&pen);
        canvas.draw_path(&outline(style, size, o.offset + o.width * 0.5));
        canvas.detach_pen();
        canvas.restore();
    }
}
fn padding_outline(style: &ViewStyle, size: Size, spread: f32) -> Path {
    let [top, right, bottom, left] = style.borders.widths.map(|width| width + spread);
    let radii = nsh_core::canvas::fit_radii(
        size.width,
        size.height,
        style
            .corner_radius
            .map(|radius| radius.unwrap_or(style.border_radius)),
    );
    let corners = [(left, top), (right, top), (right, bottom), (left, bottom)];
    let mut path = Path::new();
    if size.width <= left + right || size.height <= top + bottom {
        return path;
    }
    nsh_core::shape::rounded_rect(
        (
            left as f64,
            top as f64,
            (size.width - right) as f64,
            (size.height - bottom) as f64,
        ),
        std::array::from_fn(|i| {
            (
                (radii[i] - corners[i].0).max(0.0) as f64,
                (radii[i] - corners[i].1).max(0.0) as f64,
            )
        }),
        style.continuous_corners,
        false,
        &mut |segment| match segment {
            nsh_core::shape::Segment::Move(x, y) => path.move_to(x as f32, y as f32),
            nsh_core::shape::Segment::Line(x, y) => path.line_to(x as f32, y as f32),
            nsh_core::shape::Segment::Cubic(values) => {
                let [a, b, c, d, e, f] = values.map(|value| value as f32);
                path.bezier_curve_to(a, b, c, d, e, f);
            }
            nsh_core::shape::Segment::Close => path.close(),
        },
    );
    path
}
pub(crate) fn outline(style: &ViewStyle, size: Size, spread: f32) -> Path {
    let mut path = Path::new();
    let w = (size.width + spread * 2.0).max(0.0);
    let h = (size.height + spread * 2.0).max(0.0);
    let radii = nsh_core::canvas::fit_radii(
        w,
        h,
        style
            .corner_radius
            .map(|r| (r.unwrap_or(style.border_radius) + spread).max(0.0)),
    );
    nsh_core::shape::rounded_rect(
        (
            -spread as f64,
            -spread as f64,
            (size.width + spread) as f64,
            (size.height + spread) as f64,
        ),
        radii.map(|r| (r as f64, r as f64)),
        style.continuous_corners,
        false,
        &mut |s| match s {
            nsh_core::shape::Segment::Move(x, y) => path.move_to(x as f32, y as f32),
            nsh_core::shape::Segment::Line(x, y) => path.line_to(x as f32, y as f32),
            nsh_core::shape::Segment::Cubic(v) => {
                let [a, b, c, d, e, f] = v.map(|v| v as f32);
                path.bezier_curve_to(a, b, c, d, e, f);
            }
            nsh_core::shape::Segment::Close => path.close(),
        },
    );
    path
}
pub(crate) fn drawing_blend(mode: nsh_core::BlendMode) -> ohos_drawing_binding::BlendMode {
    use nsh_core::BlendMode as M;
    use ohos_drawing_binding::BlendMode as B;
    match mode {
        M::Normal | M::PlusDarker => B::SrcOver,
        M::Multiply => B::Multiply,
        M::Screen => B::Screen,
        M::Overlay => B::Overlay,
        M::Darken => B::Darken,
        M::Lighten => B::Lighten,
        M::ColorDodge => B::ColorDodge,
        M::ColorBurn => B::ColorBurn,
        M::HardLight => B::HardLight,
        M::SoftLight => B::SoftLight,
        M::Difference => B::Difference,
        M::Exclusion => B::Exclusion,
        M::Hue => B::Hue,
        M::Saturation => B::Saturation,
        M::Color => B::Color,
        M::Luminosity => B::Luminosity,
        M::PlusLighter => B::Plus,
    }
}
fn gradient(g: &Gradient, size: Size) -> Brush {
    let mut brush = Brush::new();
    brush.set_anti_alias(true);
    if g.stops.is_empty() {
        return brush;
    }
    let stops = g.resolved_stops();
    let first = stops[0].1;
    let last = stops.last().unwrap().1;
    let span = (last - first).max(1e-6);
    brush.set_color(argb(stops.last().unwrap().0));
    let colors: Vec<_> = stops.iter().map(|(c, _)| argb(*c)).collect();
    let positions: Vec<_> = stops.iter().map(|(_, p)| (*p - first) / span).collect();
    let tile = if g.repeating {
        TileMode::Repeat
    } else {
        TileMode::Clamp
    };
    let (w, h) = (size.width, size.height);
    let shader = match g.kind {
        GradientKind::Radial { circle, size, x, y } => {
            let (cx, cy) = (x * w, y * h);
            let (rx, ry) = radii(circle, size, cx, cy, w, h);
            let matrix = Matrix::from_affine(rx.max(0.01), 0.0, 0.0, ry.max(0.01), cx, cy);
            ShaderEffect::two_point_conical_gradient(
                (0.0, 0.0),
                first.max(0.0),
                (0.0, 0.0),
                last.max(0.0001),
                &colors,
                &positions,
                tile,
                Some(&matrix),
            )
        }
        GradientKind::Conic { from, x, y } => {
            let mut stops: Vec<_> = stops.into_iter().map(|(c, p)| (p, c)).collect();
            if g.repeating && span > 1e-5 {
                let original = stops.clone();
                stops.clear();
                let start = (-last / span).floor() as i32;
                let end = ((1.0 - first) / span).ceil() as i32;
                // A maliciously tiny period must not allocate unbounded stops.
                if (end - start) as usize * original.len() <= 4096 {
                    for k in start..=end {
                        stops.extend(original.iter().map(|(p, c)| (*p + k as f32 * span, *c)));
                    }
                } else {
                    stops = vec![(0.0, original[0].1), (1.0, original.last().unwrap().1)];
                }
            }
            let begin = crate::render::canvas::sample(&stops, 0.0);
            let end = crate::render::canvas::sample(&stops, 1.0);
            stops.retain(|(p, _)| *p > 0.0 && *p < 1.0);
            stops.insert(0, (0.0, begin));
            stops.push((1.0, end));
            let (colors, positions) =
                crate::render::canvas::sweep_stops(&stops, (from - 90.0).to_radians());
            ShaderEffect::conic_gradient((x * w, y * h), &colors, &positions, TileMode::Clamp)
        }
        kind => {
            let angle = kind.linear_angle(w, h).unwrap_or(180.0).to_radians();
            let (dx, dy) = (angle.sin(), -angle.cos());
            let length = (w * dx).abs() + (h * dy).abs();
            let point = |t: f32| {
                (
                    w * 0.5 + dx * length * (t - 0.5),
                    h * 0.5 + dy * length * (t - 0.5),
                )
            };
            ShaderEffect::linear_gradient(
                point(first),
                point(last.max(first + 1e-6)),
                &colors,
                &positions,
                tile,
            )
        }
    };
    if shader.is_some() {
        brush.set_color(0xffffffff);
        brush.set_shader_effect(shader.as_ref());
    }
    brush
}
fn radii(circle: bool, size: GradientSize, cx: f32, cy: f32, w: f32, h: f32) -> (f32, f32) {
    let near = (cx.abs().min((w - cx).abs()), cy.abs().min((h - cy).abs()));
    let far = (cx.abs().max((w - cx).abs()), cy.abs().max((h - cy).abs()));
    if circle {
        let r = match size {
            GradientSize::ClosestSide => near.0.min(near.1),
            GradientSize::FarthestSide => far.0.max(far.1),
            GradientSize::ClosestCorner => near.0.hypot(near.1),
            GradientSize::FarthestCorner => far.0.hypot(far.1),
        };
        (r, r)
    } else {
        match size {
            GradientSize::ClosestSide => near,
            GradientSize::FarthestSide => far,
            GradientSize::ClosestCorner => (
                near.0 * std::f32::consts::SQRT_2,
                near.1 * std::f32::consts::SQRT_2,
            ),
            GradientSize::FarthestCorner => (
                far.0 * std::f32::consts::SQRT_2,
                far.1 * std::f32::consts::SQRT_2,
            ),
        }
    }
}
