//! Engine-thread typography. Only owned pixels and geometry cross to the UI thread;
//! native font, paragraph and bitmap objects are created and freed here.

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use nsh_core::{AttributedText, Color, Size, TextAlign, TextEngine, TextLayout};
use ohos_drawing_binding::{
    Bitmap, BitmapFormat, Canvas, FontCollection, FontSlant, FontStyle, FontWeight, FontWidth,
    TextDirection, TextStyle, Typography, TypographyBuilder, TypographyStyle,
};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn argb(color: Color) -> u32 {
    color.0.rotate_right(8)
}

pub(crate) struct RasterText {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) origin_y: f32,
    pub(crate) pixels: Mutex<Option<Vec<u8>>>,
    pub(crate) scale: f32,
    pub(crate) lines: Vec<nsh_core::TextLine>,
    pub(crate) links: Vec<(nsh_core::TextLink, Vec<[f32; 4]>)>,
    pub(crate) attachments: Vec<Option<(f32, f32)>>,
    pub(crate) exhausted: bool,
    pub(crate) selection: Option<crate::text::selection::layout::Layout>,
    pub(crate) selection_color: Option<Color>,
    _budget: Option<Reservation>,
}

// Reserve the retained raster and one transient transfer copy together.
// A layout retained by core also retains its reservation.
const RASTER_BUDGET: usize = 96 << 20;
struct Reservation {
    used: Arc<AtomicUsize>,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.used.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}
pub(crate) struct Prepared {
    paragraph: RefCell<Typography>,
    no_wrap: bool,
    links: Vec<nsh_core::TextLink>,
    decorations: Vec<crate::text::decoration::Decoration>,
    selectable: Option<Arc<str>>,
    selection_color: Option<Color>,
}

pub(crate) struct OhosTextEngine {
    fonts: RefCell<FontCollection>,
    scale: f32,
    font_data: RefCell<Vec<Box<[u8]>>>,
    budget: Arc<AtomicUsize>,
}

impl OhosTextEngine {
    pub(crate) fn new(scale: f32) -> Self {
        Self {
            // The legacy collection supports only one paragraph builder.
            // A shared collection is reusable within this engine thread.
            fonts: RefCell::new(FontCollection::shared()),
            scale,
            font_data: RefCell::default(),
            budget: Arc::default(),
        }
    }
}

impl TextEngine for OhosTextEngine {
    type Prepared = Prepared;

    fn lines(&self, layout: &TextLayout) -> Vec<nsh_core::TextLine> {
        layout
            .0
            .downcast_ref::<RasterText>()
            .map_or_else(Vec::new, |raster| raster.lines.clone())
    }
    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        layout
            .0
            .downcast_ref::<RasterText>()
            .map_or_else(Vec::new, |raster| raster.attachments.clone())
    }
    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
        let family = face
            .names()
            .into_iter()
            .filter(|name| {
                matches!(
                    name.name_id,
                    ttf_parser::name_id::TYPOGRAPHIC_FAMILY | ttf_parser::name_id::FAMILY
                )
            })
            .find_map(|name| name.to_string())
            .filter(|name| !name.is_empty() && !name.contains('\0'))
            .ok_or("font has no valid family name")?;
        let mut bytes = data.to_vec().into_boxed_slice();
        self.fonts
            .borrow_mut()
            .register_font_buffer(&family, &mut bytes)
            .map_err(|e| e.to_string())?;
        // Retain the buffer for the font collection's lifetime, even on SDKs
        // where registration copies it internally.
        self.font_data.borrow_mut().push(bytes);
        Ok(())
    }

    fn prepare(&self, text: &AttributedText) -> Prepared {
        let mut style = TypographyStyle::new();
        style.set_text_align(match text.align {
            TextAlign::Auto => ohos_drawing_binding::TextAlign::Start,
            TextAlign::Left => ohos_drawing_binding::TextAlign::Left,
            TextAlign::Center => ohos_drawing_binding::TextAlign::Center,
            TextAlign::Right => ohos_drawing_binding::TextAlign::Right,
            TextAlign::Justify => ohos_drawing_binding::TextAlign::Justify,
        });
        style.set_text_direction(if text.writing_direction.unwrap_or(text.rtl) {
            TextDirection::Rtl
        } else {
            TextDirection::Ltr
        });
        if text.max_lines > 0 {
            style.set_max_lines(text.max_lines.min(i32::MAX as u32) as i32);
            use nsh_core::EllipsizeMode as E;
            use ohos_drawing_binding::EllipsisMode as N;
            if let Err(error) = style.set_ellipsis(
                if text.ellipsize == E::Clip { "" } else { "…" },
                match text.ellipsize {
                    E::Head => N::Head,
                    E::Middle => N::Middle,
                    _ => N::Tail,
                },
            ) {
                log::warn!("OHOS text ellipsis: {error}");
            }
        }
        let mut builder = TypographyBuilder::new(&mut style, &mut self.fonts.borrow_mut());
        // Core spans use UTF-16 offsets, including supplementary characters.
        let utf16: Vec<u16> = text.text.encode_utf16().collect();
        let mut decorations = Vec::new();
        for span in &text.spans {
            let mut style = TextStyle::new();
            let custom_decoration = (span.style.decoration_thickness.is_some()
                || span.style.underline_offset.is_some())
                && (span.style.decoration.underline || span.style.decoration.line_through);
            style.set_color(argb(span.style.color));
            let mut decoration = ohos_drawing_binding::TextDecoration::NONE;
            if span.style.decoration.underline && !custom_decoration {
                decoration |= ohos_drawing_binding::TextDecoration::UNDERLINE;
            }
            if span.style.decoration.line_through && !custom_decoration {
                decoration |= ohos_drawing_binding::TextDecoration::LINE_THROUGH;
            }
            style.set_decoration(
                decoration,
                argb(span.style.decoration_color.unwrap_or(span.style.color)),
                match span.style.decoration_style {
                    nsh_core::DecorationStyle::Solid => {
                        ohos_drawing_binding::DecorationPattern::Solid
                    }
                    nsh_core::DecorationStyle::Double => {
                        ohos_drawing_binding::DecorationPattern::Double
                    }
                    nsh_core::DecorationStyle::Dotted => {
                        ohos_drawing_binding::DecorationPattern::Dotted
                    }
                    nsh_core::DecorationStyle::Dashed => {
                        ohos_drawing_binding::DecorationPattern::Dashed
                    }
                    nsh_core::DecorationStyle::Wavy => {
                        ohos_drawing_binding::DecorationPattern::Wavy
                    }
                },
            );
            if let Some(shadow) = text.shadow {
                style.add_shadow(&ohos_drawing_binding::TextShadow::new(
                    argb(shadow.color),
                    &ohos_drawing_binding::Point::new(shadow.offset_x, shadow.offset_y),
                    shadow.radius as f64,
                ));
            }
            style.set_font_size(span.style.font_size as f64);
            style.set_font_style(FontStyle::new(
                FontWeight::from_css(span.style.font_weight),
                FontWidth::Normal,
                if span.style.italic {
                    FontSlant::Italic
                } else {
                    FontSlant::Normal
                },
            ));
            style.set_letter_spacing(span.style.letter_spacing as f64);
            for tag in span.style.font_variant.tags() {
                style.add_font_feature(tag, 1);
            }
            if let Some(family) = span.style.font_family {
                let family = family.name();
                if !family.contains('\0') {
                    style.set_font_families(&[&family]);
                }
            }
            if span.style.line_height > 0.0
                && span.style.font_size > 0.0
                && let Err(error) = style
                    .set_font_height(span.style.line_height as f64 / span.style.font_size as f64)
            {
                log::warn!("OHOS text line height: {error}");
            }
            builder.push_text_style(&mut style);
            let mut start = span.start as usize;
            for attachment in text
                .attachments
                .iter()
                .filter(|a| a.index >= span.start && a.index < span.end)
            {
                let at = attachment.index as usize;
                builder.add_text(&String::from_utf16_lossy(&utf16[start..at]));
                if let Err(error) =
                    builder.add_placeholder(&ohos_drawing_binding::PlaceholderSpan {
                        width: attachment.width as f64,
                        height: attachment.height as f64,
                        // The drawing ABI's above-baseline/alphabetic alignment
                        // preserves the existing inline attachment placement.
                        alignment: ohos_drawing_binding::PlaceholderAlignment::AboveBaseline.into(),
                        baseline: ohos_drawing_binding::PlaceholderBaseline::Alphabetic.into(),
                        baselineOffset: 0.0,
                    })
                {
                    log::warn!("OHOS text attachment: {error}");
                }
                start = at + 1;
            }
            builder.add_text(&String::from_utf16_lossy(&utf16[start..span.end as usize]));
            builder.pop_text_style();
            if custom_decoration {
                decorations.push(crate::text::decoration::Decoration::new(
                    span.start, span.end, span.style, style,
                ));
            }
        }
        Prepared {
            paragraph: RefCell::new(builder.build()),
            no_wrap: text.no_wrap,
            links: text.links.clone(),
            decorations,
            selectable: (text.selectable == Some(true)).then(|| Arc::from(text.text.as_str())),
            selection_color: text.selection_color,
        }
    }

    fn measure(&self, prepared: &Prepared, max_width: Option<f32>) -> Size {
        let mut paragraph = prepared.paragraph.borrow_mut();
        let width = if prepared.no_wrap { None } else { max_width };
        paragraph.layout(width.unwrap_or(1_000_000.0).max(0.0) as f64);
        Size::new(
            paragraph.longest_line().ceil() as f32,
            paragraph.height().ceil() as f32,
        )
    }

    fn layout(&self, prepared: &Prepared, size: Size) -> TextLayout {
        let mut paragraph = prepared.paragraph.borrow_mut();
        paragraph.layout(if prepared.no_wrap {
            1_000_000.0
        } else {
            size.width as f64
        });
        let (mut top, mut bottom) = (0.0f32, size.height);
        for decoration in &prepared.decorations {
            if let Some((min, max)) = decoration.vertical_bounds(&paragraph, self.scale) {
                top = top.min(min);
                bottom = bottom.max(max);
            }
        }
        let padding_top = (-top * self.scale).ceil() / self.scale;
        let (width, height, scale) =
            super::raster::dimensions(size.width, bottom + padding_top, self.scale);
        let mut raster = RasterText {
            width,
            height,
            origin_y: -padding_top,
            pixels: Mutex::new(None),
            scale,
            lines: Vec::new(),
            links: Vec::new(),
            attachments: Vec::new(),
            exhausted: false,
            selection: None,
            selection_color: prepared.selection_color,
            _budget: None,
        };
        // Geometry remains available even if this block cannot obtain a raster.
        raster.lines = (0..paragraph.line_count().min(i32::MAX as usize))
            .filter_map(|index| paragraph.line_metrics(index as i32))
            .map(|line| nsh_core::TextLine {
                text: String::new(),
                start: line.start_index as u32,
                end: line.end_index as u32,
                x: line.x as f32,
                y: line.y as f32,
                width: line.width as f32,
                height: line.height as f32,
                ascender: line.ascender as f32,
                descender: line.descender as f32,
                cap_height: line.cap_height as f32,
                x_height: line.x_height as f32,
            })
            .collect();
        raster.links = prepared
            .links
            .iter()
            .map(|link| {
                (
                    *link,
                    paragraph.range_rectangles(link.start as usize, link.end as usize),
                )
            })
            .collect();
        raster.attachments = paragraph
            .placeholder_rectangles()
            .into_iter()
            .map(|rect| {
                (rect[3] > rect[1] && rect[1] < size.height && rect[0] < size.width)
                    .then_some((rect[0], rect[1]))
            })
            .collect();
        if width > 0 && height > 0 {
            let selectable = prepared.selectable.as_ref().filter(|text| {
                text.len() <= crate::text::selection::layout::MAX_TEXT_BYTES
                    && text
                        .graphemes(true)
                        .take(crate::text::selection::layout::MAX_CLUSTERS + 1)
                        .count()
                        <= crate::text::selection::layout::MAX_CLUSTERS
            });
            let bytes = width as usize * height as usize * 8;
            if self
                .budget
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                    used.checked_add(bytes)
                        .filter(|next| *next <= RASTER_BUDGET)
                })
                .is_err()
            {
                raster.exhausted = true;
                return TextLayout(Arc::new(raster));
            }
            raster._budget = Some(Reservation {
                used: self.budget.clone(),
                bytes,
            });
            if let Some(text) = selectable {
                raster.selection = crate::text::selection::layout::Layout::new(
                    text.clone(),
                    size.width,
                    size.height,
                    |range| {
                        paragraph
                            .range_boxes(range.start, range.end)
                            .into_iter()
                            .map(|r| crate::text::selection::layout::Region {
                                bounds: r.bounds,
                                rtl: r.rtl,
                            })
                            .collect()
                    },
                );
                if raster.selection.is_none() {
                    log::warn!("OHOS text selection cache limit; rendering without selection");
                }
            }
            if let Some(selection) = &raster.selection {
                let bytes = selection.storage_bytes();
                if self
                    .budget
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                        used.checked_add(bytes).filter(|n| *n <= RASTER_BUDGET)
                    })
                    .is_ok()
                {
                    raster._budget.as_mut().unwrap().bytes += bytes;
                } else {
                    raster.selection = None;
                    log::warn!("OHOS text selection budget exhausted; rendering without selection");
                }
            }
            let canvas = Canvas::with_bitmap(Bitmap::new(width, height, BitmapFormat::default()));
            canvas.clear(0);
            canvas.scale(scale, scale);
            canvas.translate(0.0, padding_top);
            paragraph.paint(&canvas, 0.0, 0.0);
            for decoration in &prepared.decorations {
                decoration.draw(&canvas, &paragraph, self.scale, size.width);
            }
            let pixels = canvas.bitmap().expect("bitmap canvas").pixels();
            if pixels.len() == width as usize * height as usize * 4 {
                *raster.pixels.get_mut().expect("new text pixels") = Some(pixels.to_vec());
            } else {
                log::warn!("OHOS text bitmap allocation failed");
            }
        }
        TextLayout(Arc::new(raster))
    }
}
