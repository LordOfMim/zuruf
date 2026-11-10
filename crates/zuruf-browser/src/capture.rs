use crate::{Error, Point, Rect, Result, types::DomCapture};
use std::io::Cursor;

#[derive(Debug, Clone, Copy)]
pub struct Ring {
    pub color: [u8; 4],
    pub radius: f64,
    pub width: f64,
}
impl Default for Ring {
    fn default() -> Self {
        Self {
            color: [230, 82, 44, 255],
            radius: 20.0,
            width: 2.0,
        }
    }
}
/// Chromium DIP clip, *not* CSS pixels or raster pixels.
pub fn clip(rect: Rect, visual_offset: Point, zoom: f64) -> Result<Rect> {
    let values = [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        visual_offset.x,
        visual_offset.y,
        zoom,
    ];
    if values.iter().any(|x| !x.is_finite())
        || zoom <= 0.0
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return Err(Error::Invalid("invalid screenshot clip".into()));
    }
    Ok(Rect {
        x: (rect.x + visual_offset.x) * zoom,
        y: (rect.y + visual_offset.y) * zoom,
        width: rect.width * zoom,
        height: rect.height * zoom,
    })
}
pub fn mark(
    raw: &[u8],
    viewport: Point,
    cursor: Point,
    ring: Ring,
    outline: Option<Rect>,
) -> Result<Vec<u8>> {
    if !ring.radius.is_finite()
        || ring.radius < 2.0
        || !ring.width.is_finite()
        || ring.width <= 0.0
        || ring.width > ring.radius
    {
        return Err(Error::Invalid("invalid ring geometry".into()));
    }
    let decoder = png::Decoder::new(Cursor::new(raw));
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Invalid("PNG too large".into()))?;
    let mut bytes = vec![0; size];
    let info = reader
        .next_frame(&mut bytes)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(Error::Invalid("expected 8-bit PNG".into()));
    }
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(Error::Invalid("expected RGB/RGBA PNG".into())),
    };
    if viewport.x <= 0.0 || viewport.y <= 0.0 {
        return Err(Error::Invalid("empty viewport".into()));
    }
    let sx = f64::from(info.width) / viewport.x;
    let sy = f64::from(info.height) / viewport.y;
    let bounds = outline.unwrap_or(Rect {
        x: cursor.x - ring.radius,
        y: cursor.y - ring.radius,
        width: 2.0 * ring.radius,
        height: 2.0 * ring.radius,
    });
    let padding = ring.width + 2.0;
    let min_x = ((bounds.x - padding) * sx).floor().max(0.0) as u32;
    let max_x = ((bounds.x + bounds.width + padding) * sx)
        .ceil()
        .clamp(0.0, f64::from(info.width)) as u32;
    let min_y = ((bounds.y - padding) * sy).floor().max(0.0) as u32;
    let max_y = ((bounds.y + bounds.height + padding) * sy)
        .ceil()
        .clamp(0.0, f64::from(info.height)) as u32;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = (f64::from(x) + 0.5) / sx;
            let py = (f64::from(y) + 0.5) / sy;
            let distance = if let Some(r) = outline {
                let dx = (px - r.x).abs().min((px - r.x - r.width).abs());
                let dy = (py - r.y).abs().min((py - r.y - r.height).abs());
                if px >= r.x - 2.0
                    && px <= r.x + r.width + 2.0
                    && py >= r.y - 2.0
                    && py <= r.y + r.height + 2.0
                {
                    dx.min(dy)
                } else {
                    f64::INFINITY
                }
            } else {
                ((px - cursor.x).hypot(py - cursor.y) - ring.radius).abs()
            };
            let alpha = ((ring.width / 2.0 - distance) * sx.min(sy) + 0.5).clamp(0.0, 1.0)
                * f64::from(ring.color[3])
                / 255.0;
            let index = (y as usize * info.width as usize + x as usize) * channels;
            for c in 0..3 {
                bytes[index + c] = (f64::from(bytes[index + c]) * (1.0 - alpha)
                    + f64::from(ring.color[c]) * alpha)
                    .round() as u8;
            }
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, info.width, info.height);
        encoder.set_color(info.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fastest);
        let mut writer = encoder
            .write_header()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        writer
            .write_image_data(&bytes[..info.buffer_size()])
            .map_err(|e| Error::Invalid(e.to_string()))?;
    }
    Ok(output)
}
/// Paint configured sensitive rectangles before any image is persisted or handed off.
pub fn mask(raw: &[u8], viewport: Point, rectangles: &[Rect]) -> Result<Vec<u8>> {
    if !rectangles
        .iter()
        .any(|r| r.x < viewport.x && r.y < viewport.y && r.x + r.width > 0. && r.y + r.height > 0.)
    {
        return Ok(raw.to_vec());
    }
    let mut reader = png::Decoder::new(Cursor::new(raw))
        .read_info()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let mut bytes = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| Error::Invalid("PNG too large".into()))?
    ];
    let info = reader
        .next_frame(&mut bytes)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(Error::Invalid("RGB image required".into())),
    };
    if viewport.x <= 0. || viewport.y <= 0. {
        return Err(Error::Invalid("empty viewport".into()));
    }
    let sx = info.width as f64 / viewport.x;
    let sy = info.height as f64 / viewport.y;
    for r in rectangles {
        let left = (r.x * sx).floor().clamp(0., info.width as f64) as usize;
        let right = ((r.x + r.width) * sx).ceil().clamp(0., info.width as f64) as usize;
        let top = (r.y * sy).floor().clamp(0., info.height as f64) as usize;
        let bottom = ((r.y + r.height) * sy).ceil().clamp(0., info.height as f64) as usize;
        for y in top..bottom {
            for x in left..right {
                let offset = (y * info.width as usize + x) * channels;
                bytes[offset..offset + 3].fill(0);
                if channels == 4 {
                    bytes[offset + 3] = 255;
                }
            }
        }
    }
    let mut output = vec![];
    {
        let mut encoder = png::Encoder::new(&mut output, info.width, info.height);
        encoder.set_color(info.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fastest);
        encoder
            .write_header()
            .map_err(|e| Error::Invalid(e.to_string()))?
            .write_image_data(&bytes[..info.buffer_size()])
            .map_err(|e| Error::Invalid(e.to_string()))?;
    }
    Ok(output)
}
/// Defence in depth. The extension never sends values, HTML or arbitrary attrs.
pub fn redact(dom: &mut DomCapture) {
    dom.attributes.retain(|key, _| {
        matches!(
            key.as_str(),
            "id" | "class"
                | "role"
                | "type"
                | "data-testid"
                | "aria-label"
                | "aria-labelledby"
                | "aria-describedby"
        )
    });
    if matches!(dom.tag.as_str(), "input" | "textarea" | "select")
        || dom.attributes.get("type").is_some_and(|x| x == "password")
    {
        dom.text = None;
        dom.name = None;
        dom.locators
            .retain(|l| matches!(l.kind.as_str(), "testid" | "css" | "xpath"));
    }
    dom.locators
        .sort_by_key(|candidate| match candidate.kind.as_str() {
            "testid" => 0,
            "role" => 1,
            "text" => 2,
            "css" => 3,
            _ => 4,
        });
}
pub fn redact_url(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or_default().into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_matrix() {
        for zoom in [1.0, 1.25] {
            for dpr in [1.0, 2.0] {
                let c = clip(
                    Rect {
                        x: 10.0,
                        y: 20.0,
                        width: 80.0,
                        height: 40.0,
                    },
                    Point { x: 0.0, y: 100.0 },
                    zoom,
                )
                .unwrap();
                assert_eq!(c.width * dpr, 80.0 * zoom * dpr);
                assert_eq!(c.y, 120.0 * zoom);
            }
        }
    }
    #[test]
    fn redaction_and_ranking() {
        let mut d = DomCapture {
            tag: "input".into(),
            text: Some("secret".into()),
            ..Default::default()
        };
        d.attributes.insert("value".into(), "secret".into());
        redact(&mut d);
        assert!(d.attributes.is_empty());
        assert!(d.text.is_none());
    }
    #[test]
    fn mask_covers_scaled_rectangle_without_changing_other_pixels() {
        let mut raw = vec![];
        {
            let mut encoder = png::Encoder::new(&mut raw, 4, 4);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255; 48])
                .unwrap();
        }
        let image = mask(
            &raw,
            Point { x: 2., y: 2. },
            &[Rect {
                x: 0.5,
                y: 0.5,
                width: 1.,
                height: 1.,
            }],
        )
        .unwrap();
        let mut reader = png::Decoder::new(Cursor::new(image)).read_info().unwrap();
        let mut bytes = vec![0; 48];
        reader.next_frame(&mut bytes).unwrap();
        for y in 0..4 {
            for x in 0..4 {
                let expected = if (1..3).contains(&x) && (1..3).contains(&y) {
                    0
                } else {
                    255
                };
                assert_eq!(&bytes[(y * 4 + x) * 3..(y * 4 + x) * 3 + 3], &[expected; 3]);
            }
        }
    }
    #[test]
    fn ring_golden_pixels() {
        let mut raw = Vec::new();
        {
            let mut e = png::Encoder::new(&mut raw, 9, 9);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[255; 243])
                .unwrap();
        }
        let marked = mark(
            &raw,
            Point { x: 9.0, y: 9.0 },
            Point { x: 4.5, y: 4.5 },
            Ring {
                color: [0, 0, 0, 255],
                radius: 3.0,
                width: 1.0,
            },
            None,
        )
        .unwrap();
        let mut r = png::Decoder::new(Cursor::new(marked)).read_info().unwrap();
        let mut pixels = vec![0; 243];
        r.next_frame(&mut pixels).unwrap();
        assert_eq!(
            &pixels[(4 * 9 + 4) * 3..(4 * 9 + 4) * 3 + 3],
            &[255, 255, 255]
        );
        assert_eq!(&pixels[(4 * 9 + 7) * 3..(4 * 9 + 7) * 3 + 3], &[0, 0, 0]);
        assert_eq!(&pixels[0..3], &[255, 255, 255]);
    }
}
