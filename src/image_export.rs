use std::{
    fmt::Write,
    sync::{Arc, OnceLock},
};

use resvg::{tiny_skia, usvg};

use crate::{CELL_HEIGHT, CELL_WIDTH, INK, MONO_FONT, PAPER};

const PADDING: u32 = 12;
const BASELINE: u32 = 16;
const MAX_PNG_PIXELS: u64 = 16_000_000;
const MAX_IMAGE_GLYPHS: usize = 50_000;

pub fn svg(text: &str) -> Result<String, String> {
    let layout = Layout::new(text)?;
    // Resolve glyph outlines once so the SVG looks the same on machines without our fonts.
    Ok(layout.tree()?.to_string(&usvg::WriteOptions::default()))
}

pub fn png(text: &str) -> Result<Vec<u8>, String> {
    let layout = Layout::new(text)?;
    if u64::from(layout.width) * u64::from(layout.height) > MAX_PNG_PIXELS {
        return Err("The diagram is too large for PNG export. Use SVG instead.".to_owned());
    }

    let tree = layout.tree()?;
    let mut pixmap = tiny_skia::Pixmap::new(layout.width, layout.height)
        .ok_or_else(|| "Could not create the PNG image.".to_owned())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|error| error.to_string())
}

struct Layout<'a> {
    text: &'a str,
    width: u32,
    height: u32,
}

impl<'a> Layout<'a> {
    fn new(text: &'a str) -> Result<Self, String> {
        if text
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .take(MAX_IMAGE_GLYPHS + 1)
            .count()
            > MAX_IMAGE_GLYPHS
        {
            return Err("The diagram exceeds 50,000 visible characters. Export it as text.".into());
        }
        let mut columns = 1;
        let mut rows = 0;
        for line in text.split('\n') {
            columns = columns.max(line.chars().count());
            rows += 1;
        }

        let dimension = |cells, step| {
            u32::try_from(cells)
                .ok()
                .and_then(|cells| cells.checked_mul(step))
                .and_then(|size| size.checked_add(PADDING * 2))
                .ok_or_else(|| "The diagram is too large to export as an image.".to_owned())
        };
        Ok(Self {
            text,
            width: dimension(columns, CELL_WIDTH as u32)?,
            height: dimension(rows, CELL_HEIGHT as u32)?,
        })
    }

    fn markup(&self) -> String {
        let Self { width, height, .. } = *self;
        let font_size = CELL_HEIGHT * 0.76;
        let mut document = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="#{PAPER:06x}"/><g fill="#{INK:06x}" font-family="monospace" font-size="{font_size}" text-anchor="middle">"##
        );
        for (row, line) in self.text.split('\n').enumerate() {
            let y = PADDING + row as u32 * CELL_HEIGHT as u32 + BASELINE;
            for (column, glyph) in line.chars().enumerate() {
                if glyph.is_whitespace() {
                    continue;
                }
                let x = PADDING + column as u32 * CELL_WIDTH as u32 + CELL_WIDTH as u32 / 2;
                write!(document, r#"<text x="{x}" y="{y}">"#).unwrap();
                match glyph {
                    '<' => document.push_str("&lt;"),
                    '>' => document.push_str("&gt;"),
                    '&' => document.push_str("&amp;"),
                    '\u{fffe}' | '\u{ffff}' => document.push('\u{fffd}'),
                    ch if ch.is_control() => document.push('\u{fffd}'),
                    ch => document.push(ch),
                }
                document.push_str("</text>");
            }
        }
        document.push_str("</g></svg>");
        document
    }

    fn tree(&self) -> Result<usvg::Tree, String> {
        let options = usvg::Options {
            fontdb: Arc::clone(system_fonts()?),
            ..usvg::Options::default()
        };
        usvg::Tree::from_str(&self.markup(), &options).map_err(|error| error.to_string())
    }
}

fn system_fonts() -> Result<&'static Arc<usvg::fontdb::Database>, String> {
    static FONTS: OnceLock<Option<Arc<usvg::fontdb::Database>>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            use usvg::fontdb::Database;

            let mut database = Database::new();
            database.load_system_fonts();
            let preferred = [MONO_FONT, "Liberation Mono", "Courier New", "Courier"];
            let family = preferred
                .into_iter()
                .find(|name| {
                    database
                        .faces()
                        .any(|face| face.families.iter().any(|(family, _)| family == name))
                })
                .or_else(|| {
                    database
                        .faces()
                        .find(|face| face.monospaced && !face.families.is_empty())
                        .map(|face| face.families[0].0.as_str())
                })?
                .to_owned();
            database.set_monospace_family(family);
            Some(Arc::new(database))
        })
        .as_ref()
        .ok_or_else(|| "Image export needs an installed monospace font.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_preserves_cells_and_escapes_markup() {
        let layout = Layout::new("<é\n\n  &>").unwrap();
        assert_eq!((layout.width, layout.height), (72, 87));
        let markup = layout.markup();
        assert!(markup.contains(r#"<text x="18" y="28">&lt;</text><text x="30" y="28">é</text>"#));
        assert!(
            markup.contains(r#"<text x="42" y="70">&amp;</text><text x="54" y="70">&gt;</text>"#)
        );
    }

    #[test]
    fn svg_and_png_render_the_same_diagram() {
        let diagram = "X  <\n\n  &é";
        let svg = svg(diagram).unwrap();
        assert!(!svg.contains("<text"));
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let png = tiny_skia::Pixmap::decode_png(&png(diagram).unwrap()).unwrap();
        assert_eq!((png.width(), png.height()), (72, 87));
        assert_ne!(png.pixel(0, 0), None);
        assert!(
            png.pixels()
                .iter()
                .any(|pixel| Some(*pixel) != png.pixel(0, 0))
        );

        let mut from_svg = tiny_skia::Pixmap::new(png.width(), png.height()).unwrap();
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut from_svg.as_mut(),
        );
        assert_eq!(png, from_svg);
    }

    #[test]
    fn png_rejects_excessive_pixel_counts() {
        assert!(png(&"X".repeat(30_000)).is_err());
    }
}
