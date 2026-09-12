use helix_core::Position;
use helix_view::graphics::Rect;
use helix_view::hex_dump;
use helix_view::theme::Style;
use helix_view::view::ViewPosition;
use helix_view::{Document, Theme};
use tui::buffer::Buffer as Surface;

use super::document::{LinePos, TextRenderer};
use super::text_decorations::DecorationManager;

struct HexStyles {
    offset: Style,
    hex: Style,
    zero: Style,
    ascii: Style,
    dot: Style,
    gap: Style,
}

fn hex_styles(theme: &Theme) -> HexStyles {
    fn pick(theme: &Theme, keys: &[&str], fallback: &str) -> Style {
        for key in keys {
            if theme.find_highlight_exact(key).is_some() {
                return theme.get(key);
            }
        }
        theme.get(fallback)
    }

    HexStyles {
        // xxd-style: cyan address, default hex, green printable, dim dots
        offset: pick(theme, &["ui.text.hex.offset", "label", "keyword"], "ui.text.info"),
        hex: pick(theme, &["ui.text.hex.byte", "constant.numeric"], "ui.text"),
        zero: pick(
            theme,
            &["ui.text.hex.zero", "comment"],
            "ui.text.inactive",
        ),
        ascii: pick(theme, &["ui.text.hex.ascii", "string"], "ui.text"),
        dot: pick(
            theme,
            &["ui.text.hex.nonprintable", "comment"],
            "ui.text.inactive",
        ),
        gap: theme.get("ui.text"),
    }
}

pub fn render_hex_dump(
    surface: &mut Surface,
    viewport: Rect,
    doc: &Document,
    offset: ViewPosition,
    theme: &Theme,
    decorations: &mut DecorationManager,
) {
    let Some(bytes) = doc.hex_bytes() else {
        return;
    };

    let styles = hex_styles(theme);
    let mut renderer = TextRenderer::new(
        surface,
        doc,
        theme,
        Position::new(offset.vertical_offset, offset.horizontal_offset),
        viewport,
    );

    let text = doc.text().slice(..);
    let anchor = offset.anchor.min(text.len_chars());
    let first_line = text.char_to_line(anchor);
    let total_lines = bytes.len().div_ceil(hex_dump::BYTES_PER_LINE);
    let visible = viewport.height as usize;

    for row in 0..visible {
        let line = first_line + row;
        if line >= total_lines {
            break;
        }

        let line_pos = LinePos {
            first_visual_line: row == 0,
            doc_line: line,
            visual_line: row as u16,
        };
        decorations.decorate_line(&mut renderer, line_pos);

        let byte_offset = line * hex_dump::BYTES_PER_LINE;
        let chunk = &bytes[byte_offset..(byte_offset + hex_dump::BYTES_PER_LINE).min(bytes.len())];
        let formatted = hex_dump::format_line(byte_offset, chunk);
        let y = viewport.y + row as u16;

        renderer.set_stringn(
            viewport.x,
            y,
            &formatted[..8.min(formatted.len())],
            8,
            styles.offset,
        );
        if formatted.len() > 8 {
            renderer.set_stringn(
                viewport.x + 8,
                y,
                &formatted[8..10.min(formatted.len())],
                2,
                styles.gap,
            );
        }

        for byte_in_line in 0..hex_dump::BYTES_PER_LINE {
            if byte_in_line >= chunk.len() {
                break;
            }
            let col = hex_dump::hex_col(byte_in_line, 0) as u16;
            let byte = chunk[byte_in_line];
            let style = if byte == 0 { styles.zero } else { styles.hex };
            if col as usize + 2 <= formatted.len() {
                renderer.set_stringn(
                    viewport.x + col,
                    y,
                    &formatted[col as usize..col as usize + 2],
                    2,
                    style,
                );
            }
        }

        for (byte_in_line, &byte) in chunk.iter().enumerate() {
            let col = hex_dump::ASCII_START + byte_in_line;
            if col >= formatted.len() {
                break;
            }
            let style = if byte.is_ascii_graphic() || byte == b' ' {
                styles.ascii
            } else {
                styles.dot
            };
            renderer.set_stringn(
                viewport.x + col as u16,
                y,
                &formatted[col..col + 1],
                1,
                style,
            );
        }
    }
}
