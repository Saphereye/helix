use helix_core::Position;
use helix_view::editor::CursorCache;
use helix_view::graphics::Rect;
use helix_view::hex_dump::{self, ByteCategory};
use helix_view::theme::Style;
use helix_view::view::ViewPosition;
use helix_view::{Document, Theme};
use tui::buffer::Buffer as Surface;

use super::document::{LinePos, TextRenderer};
use super::text_decorations::DecorationManager;

struct HexStyles {
    offset: Style,
    null: Style,
    ascii_printable: Style,
    ascii_whitespace: Style,
    ascii_other: Style,
    nonascii: Style,
    gap: Style,
}

fn pick_style(theme: &Theme, env_name: &str, theme_keys: &[&str], default: Style) -> Style {
    if let Some(style) = hex_dump::hexyl_env_style(env_name) {
        return style;
    }
    for key in theme_keys {
        if theme.find_highlight_exact(key).is_some() {
            return theme.get(key);
        }
    }
    default
}

fn hex_styles(theme: &Theme) -> HexStyles {
    let text = theme.get("ui.text");
    HexStyles {
        offset: pick_style(
            theme,
            "HEXYL_COLOR_OFFSET",
            &["ui.text.hex.offset", "label"],
            Style::default().fg(helix_view::graphics::Color::Gray),
        ),
        null: pick_style(
            theme,
            "HEXYL_COLOR_NULL",
            &["ui.text.hex.null", "comment"],
            Style::default().fg(helix_view::graphics::Color::Gray),
        ),
        ascii_printable: pick_style(
            theme,
            "HEXYL_COLOR_ASCII_PRINTABLE",
            &["ui.text.hex.ascii.printable", "string"],
            Style::default().fg(helix_view::graphics::Color::Cyan),
        ),
        ascii_whitespace: pick_style(
            theme,
            "HEXYL_COLOR_ASCII_WHITESPACE",
            &["ui.text.hex.ascii.whitespace"],
            Style::default().fg(helix_view::graphics::Color::Green),
        ),
        ascii_other: pick_style(
            theme,
            "HEXYL_COLOR_ASCII_OTHER",
            &["ui.text.hex.ascii.other"],
            Style::default().fg(helix_view::graphics::Color::Green),
        ),
        nonascii: pick_style(
            theme,
            "HEXYL_COLOR_NONASCII",
            &["ui.text.hex.nonascii", "constant.numeric"],
            Style::default().fg(helix_view::graphics::Color::Yellow),
        ),
        gap: text,
    }
}

fn style_for_category(styles: &HexStyles, category: ByteCategory) -> Style {
    match category {
        ByteCategory::Null => styles.null,
        ByteCategory::AsciiPrintable => styles.ascii_printable,
        ByteCategory::AsciiWhitespace => styles.ascii_whitespace,
        ByteCategory::AsciiOther => styles.ascii_other,
        ByteCategory::NonAscii => styles.nonascii,
    }
}

fn render_hex_line(
    renderer: &mut TextRenderer,
    viewport_x: u16,
    y: u16,
    byte_offset: usize,
    chunk: &[u8],
    styles: &HexStyles,
) {
    let offset = format!("{byte_offset:08x}");
    renderer.set_stringn(viewport_x, y, &offset, 8, styles.offset);
    renderer.set_stringn(viewport_x + 8, y, "  ", 2, styles.gap);

    for byte_in_line in 0..hex_dump::BYTES_PER_LINE {
        let col = hex_dump::hex_col(byte_in_line, 0) as u16;
        if byte_in_line < chunk.len() {
            let byte = chunk[byte_in_line];
            let pair = format!("{byte:02x}");
            renderer.set_stringn(
                viewport_x + col,
                y,
                &pair,
                2,
                style_for_category(styles, hex_dump::byte_category(byte)),
            );
        } else {
            renderer.set_stringn(viewport_x + col, y, "   ", 3, styles.gap);
        }
    }

    renderer.set_stringn(viewport_x + (hex_dump::ASCII_START - 1) as u16, y, " ", 1, styles.gap);

    for byte_in_line in 0..hex_dump::BYTES_PER_LINE {
        let col = hex_dump::ASCII_START + byte_in_line;
        if byte_in_line < chunk.len() {
            let byte = chunk[byte_in_line];
            let ch = hex_dump::ascii_char(byte).to_string();
            renderer.set_stringn(
                viewport_x + col as u16,
                y,
                &ch,
                1,
                style_for_category(styles, hex_dump::byte_category(byte)),
            );
        } else {
            renderer.set_stringn(viewport_x + col as u16, y, " ", 1, styles.gap);
        }
    }
}

pub fn render_hex_dump(
    surface: &mut Surface,
    viewport: Rect,
    doc: &Document,
    offset: ViewPosition,
    theme: &Theme,
    decorations: &mut DecorationManager,
    primary_cursor: usize,
    cursor_cache: &CursorCache,
    draw_block_cursor: bool,
    cursor_style: Style,
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

    let byte_len = bytes.len();
    let max_char = hex_dump::display_len_chars(byte_len).saturating_sub(1);
    let anchor = offset.anchor.min(max_char);
    let (first_line, _) = hex_dump::char_to_line_col(anchor, byte_len);
    let total_lines = hex_dump::line_count(byte_len);
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
        render_hex_line(
            &mut renderer,
            viewport.x,
            viewport.y + row as u16,
            byte_offset,
            chunk,
            &styles,
        );
    }

    if let Some(pos) = hex_dump::cursor_screen_pos(
        byte_len,
        offset,
        primary_cursor,
        viewport.height as usize,
        viewport.width as usize,
    ) {
        cursor_cache.set(Some(pos));
        if draw_block_cursor {
            let x = viewport.x + pos.col as u16;
            let y = viewport.y + pos.row as u16;
            if let Some(cell) = surface.get_mut(x, y) {
                cell.set_style(cursor_style);
            }
        }
    } else {
        cursor_cache.set(None);
    }
}
