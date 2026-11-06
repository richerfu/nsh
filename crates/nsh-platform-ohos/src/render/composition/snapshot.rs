//! A navigation side captured without redirecting its native render tree.

use ohos_image_native_binding::types::{PixelFormat, PixelMapAlphaType};

use super::readback::RgbaFrame;
use crate::render::node::{Error, Node, Result};

pub(crate) struct Snapshot {
    pub(crate) frame: RgbaFrame,
    pub(crate) origin: (i32, i32),
}

impl Snapshot {
    /// Consume the one-time pixels into a destination's physical screen bounds.
    pub(super) fn take_aligned(
        &mut self,
        width: u32,
        height: u32,
        origin: (i32, i32),
    ) -> RgbaFrame {
        let image = super::filter::Image::new(RgbaFrame {
            width: self.frame.width,
            height: self.frame.height,
            pixels: std::mem::take(&mut self.frame.pixels),
        })
        .aligned(
            width,
            height,
            (self.origin.0 - origin.0, self.origin.1 - origin.1),
        );
        RgbaFrame {
            width,
            height,
            pixels: image.bitmap.pixels().to_vec(),
        }
    }

    pub(crate) fn capture(node: &Node) -> Result<Self> {
        // A texture-export wrapper's visible content is its presentation
        // image. Capture that node directly: ArkUI can skip to the first
        // child of a layout-only wrapper, which is the exported input tree.
        let mounted = node
            .wrapper
            .borrow()
            .as_ref()
            .and_then(|wrapper| {
                wrapper
                    .composite
                    .borrow()
                    .as_ref()
                    .and_then(|composite| composite.presented_node())
            })
            .unwrap_or_else(|| node.mounted());
        let native = mounted.borrow();
        let origin = native.layout_position_in_screen()?;
        let pixels = native.snapshot()?;
        drop(native);
        let read = || -> std::result::Result<RgbaFrame, String> {
            let info = pixels.image_info().map_err(|e| e.to_string())?;
            let width = info.width().map_err(|e| e.to_string())?;
            let height = info.height().map_err(|e| e.to_string())?;
            let stride = info.row_stride().map_err(|e| e.to_string())? as usize;
            let format = info.pixel_format().map_err(|e| e.to_string())?;
            let alpha = info.alpha_type().map_err(|e| e.to_string())?;
            if width == 0
                || height == 0
                || width > nsh_engine::shader::MAX_SIDE
                || height > nsh_engine::shader::MAX_SIDE
                || stride < width as usize * 4
                || stride > nsh_engine::shader::MAX_SIDE as usize * 8
                || !matches!(format, PixelFormat::Rgba8888 | PixelFormat::Bgra8888)
            {
                return Err("unsupported navigation snapshot layout".into());
            }
            let mut source = vec![0; stride * height as usize];
            let count = pixels.read_pixels(&mut source).map_err(|e| e.to_string())?;
            if count < stride * height as usize {
                return Err("incomplete navigation snapshot".into());
            }
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for row in source.chunks_exact(stride) {
                rgba.extend_from_slice(&row[..width as usize * 4]);
            }
            for pixel in rgba.as_chunks_mut::<4>().0 {
                if format == PixelFormat::Bgra8888 {
                    pixel.swap(0, 2);
                }
                match alpha {
                    PixelMapAlphaType::Opaque => pixel[3] = 255,
                    PixelMapAlphaType::Unpremultiplied => {
                        let a = u16::from(pixel[3]);
                        for channel in &mut pixel[..3] {
                            *channel = ((u16::from(*channel) * a + 127) / 255) as u8;
                        }
                    }
                    _ => {}
                }
            }
            Ok(RgbaFrame {
                width,
                height,
                pixels: rgba,
            })
        };
        let frame = read().map_err(|error| Error::from(napi_ohos::Error::from_reason(error)))?;
        Ok(Self {
            frame,
            origin: (origin.x, origin.y),
        })
    }
}
