use std::io::{self, Read};
use std::path::Path;

use helix_core::{Position, Range, Rope, RopeSlice, Selection};

use crate::graphics::{Color, Style};
use crate::view::{View, ViewPosition};
use crate::Document;

pub const BYTES_PER_LINE: usize = 16;
/// `"00000000  "`
pub const OFFSET_END: usize = 10;
/// First column of the ASCII column (after `"  "` separator).
pub const ASCII_START: usize = 60;
/// Characters per formatted dump line (excluding newline).
pub const LINE_WIDTH: usize = ASCII_START + BYTES_PER_LINE;

/// Byte categories matching [hexyl](https://github.com/sharkdp/hexyl)'s color groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteCategory {
    Null,
    AsciiPrintable,
    AsciiWhitespace,
    AsciiOther,
    NonAscii,
}

pub fn byte_category(byte: u8) -> ByteCategory {
    match byte {
        0 => ByteCategory::Null,
        0x01..=0x7f if byte.is_ascii_whitespace() => ByteCategory::AsciiWhitespace,
        0x01..=0x7f if byte.is_ascii_graphic() => ByteCategory::AsciiPrintable,
        0x01..=0x7f => ByteCategory::AsciiOther,
        _ => ByteCategory::NonAscii,
    }
}

/// Parse a hexyl-style color (`blue`, `bright green`, `#abcdef`).
pub fn parse_hexyl_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.starts_with('#') {
        return Color::from_hex(value).ok();
    }
    let lower = value.to_ascii_lowercase();
    let (bright, name) = lower
        .strip_prefix("bright ")
        .map_or((false, lower.as_str()), |name| (true, name));
    Some(match name {
        "black" => {
            if bright {
                Color::Gray
            } else {
                Color::Black
            }
        }
        "red" => {
            if bright {
                Color::LightRed
            } else {
                Color::Red
            }
        }
        "green" => {
            if bright {
                Color::LightGreen
            } else {
                Color::Green
            }
        }
        "yellow" => {
            if bright {
                Color::LightYellow
            } else {
                Color::Yellow
            }
        }
        "blue" => {
            if bright {
                Color::LightBlue
            } else {
                Color::Blue
            }
        }
        "magenta" => {
            if bright {
                Color::LightMagenta
            } else {
                Color::Magenta
            }
        }
        "cyan" => {
            if bright {
                Color::LightCyan
            } else {
                Color::Cyan
            }
        }
        "white" => {
            if bright {
                Color::White
            } else {
                Color::LightGray
            }
        }
        _ => return None,
    })
}

pub fn hexyl_env_style(env_name: &str) -> Option<Style> {
    std::env::var(env_name)
        .ok()
        .and_then(|value| parse_hexyl_color(&value).map(|color| Style::default().fg(color)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexField {
    Offset,
    Hex { byte_in_line: usize, nibble: u8 },
    Ascii { byte_in_line: usize },
    Gap,
}

/// Heuristic binary check over a leading chunk (NUL byte or known magic).
pub fn is_binary(buffer: &[u8]) -> bool {
    const BYTE_ORDER_MARKS: &[&[u8]] = &[
        &[0xEF, 0xBB, 0xBF],
        &[0x00, 0x00, 0xFE, 0xFF],
        &[0xFF, 0xFE, 0x00, 0x00],
        &[0xFE, 0xFF],
        &[0xFF, 0xFE],
    ];

    if BYTE_ORDER_MARKS.iter().any(|bom| buffer.starts_with(bom)) {
        return false;
    }

    let scan = &buffer[..buffer.len().min(1024)];
    scan.contains(&0) || buffer.starts_with(b"%PDF") || buffer.starts_with(b"\x89PNG")
}

pub fn read_file_bytes(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

pub fn line_count(byte_len: usize) -> usize {
    if byte_len == 0 {
        1
    } else {
        byte_len.div_ceil(BYTES_PER_LINE)
    }
}

/// Virtual document length matching `bytes_to_rope` layout without materializing lines.
pub fn display_len_chars(byte_len: usize) -> usize {
    let lines = line_count(byte_len);
    lines * (LINE_WIDTH + 1) - 1
}

pub fn line_to_char(line: usize, byte_len: usize) -> usize {
    let lines = line_count(byte_len);
    line.min(lines.saturating_sub(1)) * (LINE_WIDTH + 1)
}

pub fn line_col_to_char(line: usize, col: usize, byte_len: usize) -> usize {
    let lines = line_count(byte_len);
    let line = line.min(lines.saturating_sub(1));
    let col = col.min(LINE_WIDTH - 1);
    line * (LINE_WIDTH + 1) + col
}

pub fn char_to_line_col(char_idx: usize, byte_len: usize) -> (usize, usize) {
    let max_char = display_len_chars(byte_len).saturating_sub(1);
    let char_idx = char_idx.min(max_char);
    let lines = line_count(byte_len);
    let per_line = LINE_WIDTH + 1;
    let mut line = char_idx / per_line;
    let mut col = char_idx % per_line;
    if line >= lines {
        line = lines - 1;
        col = LINE_WIDTH - 1;
    } else if col >= LINE_WIDTH {
        if line + 1 < lines {
            line += 1;
            col = 0;
        } else {
            col = LINE_WIDTH - 1;
        }
    }
    (line, col)
}

pub fn clamp_selection(selection: Selection, byte_len: usize) -> Selection {
    let max = display_len_chars(byte_len).saturating_sub(1);
    selection.transform(|range| Range {
        anchor: range.anchor.min(max),
        head: range.head.min(max),
        old_visual_position: range.old_visual_position,
    })
}

pub fn move_range(
    range: Range,
    dir: helix_core::movement::Direction,
    count: usize,
    behaviour: helix_core::movement::Movement,
    byte_len: usize,
    vertical: bool,
) -> Range {
    use helix_core::movement::{Direction, Movement};

    let max = display_len_chars(byte_len).saturating_sub(1);
    let cursor = range.head.min(max);
    let (line, col) = char_to_line_col(cursor, byte_len);
    let lines = line_count(byte_len);
    let (new_line, new_col) = if vertical {
        match dir {
            Direction::Forward => ((line + count).min(lines.saturating_sub(1)), col),
            Direction::Backward => (line.saturating_sub(count), col),
        }
    } else {
        match dir {
            Direction::Forward => {
                let mut c = col.saturating_add(count);
                let mut l = line;
                while c >= LINE_WIDTH && l + 1 < lines {
                    c -= LINE_WIDTH;
                    l += 1;
                }
                if c >= LINE_WIDTH {
                    c = LINE_WIDTH - 1;
                }
                (l, c)
            }
            Direction::Backward => {
                let mut c = col;
                let mut l = line;
                for _ in 0..count {
                    if c == 0 {
                        if l == 0 {
                            break;
                        }
                        l -= 1;
                        c = LINE_WIDTH - 1;
                    } else {
                        c -= 1;
                    }
                }
                (l, c)
            }
        }
    };

    let cursor = line_col_to_char(new_line, new_col, byte_len);
    match behaviour {
        Movement::Move => Range::point(cursor),
        Movement::Extend => Range {
            anchor: range.anchor,
            head: cursor,
            old_visual_position: None,
        },
    }
}

/// Scroll the hex viewport by `line_count` lines.
pub fn scroll_view(
    view_offset: ViewPosition,
    byte_len: usize,
    line_delta: isize,
) -> ViewPosition {
    let max_char = display_len_chars(byte_len).saturating_sub(1);
    let lines = line_count(byte_len);
    let (anchor_line, _) = char_to_line_col(view_offset.anchor.min(max_char), byte_len);
    let new_line = (anchor_line as isize + line_delta).clamp(0, lines as isize - 1) as usize;
    ViewPosition {
        anchor: line_to_char(new_line, byte_len),
        vertical_offset: 0,
        horizontal_offset: view_offset.horizontal_offset,
    }
}

/// Move a cursor by `line_delta` lines, keeping its column.
pub fn move_cursor_lines(
    range: Range,
    byte_len: usize,
    line_delta: isize,
    extend: bool,
) -> Range {
    let max = display_len_chars(byte_len).saturating_sub(1);
    let cursor = range.head.min(max);
    let (line, col) = char_to_line_col(cursor, byte_len);
    let lines = line_count(byte_len);
    let new_line = (line as isize + line_delta).clamp(0, lines as isize - 1) as usize;
    let new_cursor = line_col_to_char(new_line, col, byte_len);
    if extend {
        Range {
            anchor: range.anchor,
            head: new_cursor,
            old_visual_position: None,
        }
    } else {
        Range::point(new_cursor)
    }
}

/// Keep the primary cursor visible in the hex viewport (soft-wrap disabled).
pub fn scroll_to_cursor(
    doc: &Document,
    view: &View,
    scrolloff: usize,
    center: bool,
) -> Option<ViewPosition> {
    let bytes = doc.hex_bytes()?;
    let byte_len = bytes.len();
    let view_offset = doc.get_view_offset(view.id)?;
    let viewport = view.inner_area(doc);
    let cursor = doc.display_cursor(doc.selection(view.id).primary());
    let max_char = display_len_chars(byte_len).saturating_sub(1);
    let cursor = cursor.min(max_char);

    let (line, col) = char_to_line_col(cursor, byte_len);
    let anchor = view_offset.anchor.min(max_char);
    let (anchor_line, _) = char_to_line_col(anchor, byte_len);

    let (scrolloff_top, scrolloff_bottom) = if center {
        (0, 0)
    } else {
        (
            scrolloff.min(viewport.height.saturating_sub(1) as usize / 2),
            scrolloff.min(viewport.height as usize / 2),
        )
    };
    let (scrolloff_left, scrolloff_right) = if center {
        (0, 0)
    } else {
        (
            scrolloff.min(viewport.width.saturating_sub(1) as usize / 2),
            scrolloff.min(viewport.width as usize / 2),
        )
    };

    let mut offset = view_offset;
    let screen_row = line.saturating_sub(anchor_line);
    let vertical_end = offset.vertical_offset + viewport.height as usize;

    let new_anchor = if center {
        screen_row != viewport.height as usize / 2
    } else if screen_row < scrolloff_top + offset.vertical_offset {
        true
    } else if screen_row + scrolloff_bottom >= vertical_end {
        true
    } else {
        false
    };

    if new_anchor {
        let target_row = if center {
            viewport.height as usize / 2
        } else if screen_row < scrolloff_top + offset.vertical_offset {
            scrolloff_top
        } else {
            viewport.height as usize - scrolloff_bottom - 1
        };
        let first_line = line.saturating_sub(target_row);
        offset.anchor = line_to_char(first_line, byte_len);
        offset.vertical_offset = 0;
    }

    let screen_col = col;
    let last_col = offset.horizontal_offset + viewport.width.saturating_sub(1) as usize;
    if screen_col > last_col.saturating_sub(scrolloff_right) {
        offset.horizontal_offset += screen_col - last_col.saturating_sub(scrolloff_right);
    } else if screen_col < offset.horizontal_offset + scrolloff_left {
        offset.horizontal_offset = screen_col.saturating_sub(scrolloff_left);
    }

    if !center && offset == view_offset {
        None
    } else {
        Some(offset)
    }
}

pub fn format_line_for_bytes(bytes: &[u8], line: usize) -> String {
    let offset = line * BYTES_PER_LINE;
    let end = (offset + BYTES_PER_LINE).min(bytes.len());
    format_line(offset, &bytes[offset..end])
}

pub fn bytes_to_rope(bytes: &[u8]) -> Rope {
    let lines = line_count(bytes.len());
    let mut formatted = Vec::with_capacity(lines);
    for line in 0..lines {
        formatted.push(format_line_for_bytes(bytes, line));
    }
    Rope::from(formatted.join("\n"))
}

pub fn format_line(offset: usize, chunk: &[u8]) -> String {
    let mut line = format!("{offset:08x}  ");
    for i in 0..BYTES_PER_LINE {
        if i == 8 {
            line.push(' ');
        }
        if let Some(&byte) = chunk.get(i) {
            use std::fmt::Write;
            let _ = write!(line, "{byte:02x} ");
        } else {
            line.push_str("   ");
        }
    }
    line.push(' ');
    for i in 0..BYTES_PER_LINE {
        if let Some(&byte) = chunk.get(i) {
            line.push(ascii_char(byte));
        } else {
            line.push(' ');
        }
    }
    line
}

pub fn ascii_char(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        byte as char
    } else {
        '.'
    }
}

pub fn hex_col(byte_in_line: usize, nibble: usize) -> usize {
    debug_assert!(byte_in_line < BYTES_PER_LINE);
    debug_assert!(nibble < 2);
    OFFSET_END + byte_in_line * 3 + if byte_in_line >= 8 { 1 } else { 0 } + nibble
}

pub fn field_at_col(col: usize) -> HexField {
    if col < 8 {
        HexField::Offset
    } else if col < OFFSET_END {
        HexField::Gap
    } else if col < ASCII_START {
        match col_to_hex(col) {
            Some((byte_in_line, nibble)) => HexField::Hex {
                byte_in_line,
                nibble,
            },
            None => HexField::Gap,
        }
    } else {
        let byte_in_line = col - ASCII_START;
        if byte_in_line < BYTES_PER_LINE {
            HexField::Ascii { byte_in_line }
        } else {
            HexField::Gap
        }
    }
}

/// Byte index for a hex/ASCII field on a dump line.
pub fn raw_byte_index(line: usize, byte_in_line: usize) -> usize {
    line * BYTES_PER_LINE + byte_in_line
}

/// Where to insert/overwrite: existing byte or EOF slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteSlot {
    Existing(usize),
    Eof,
}

pub fn byte_slot(raw_index: usize, len: usize) -> ByteSlot {
    if raw_index < len {
        ByteSlot::Existing(raw_index)
    } else {
        ByteSlot::Eof
    }
}

pub fn col_to_hex(col: usize) -> Option<(usize, u8)> {
    if col < OFFSET_END || col >= ASCII_START {
        return None;
    }
    let mut rel = col - OFFSET_END;
    if rel >= 24 {
        rel -= 1;
    }
    let byte_in_line = rel / 3;
    let rem = rel % 3;
    if byte_in_line >= BYTES_PER_LINE || rem == 2 {
        return None;
    }
    Some((byte_in_line, rem as u8))
}

pub fn byte_index_to_hex_cursor(byte_index: usize, nibble: usize) -> (usize, usize) {
    let line = byte_index / BYTES_PER_LINE;
    let byte_in_line = byte_index % BYTES_PER_LINE;
    (line, hex_col(byte_in_line, nibble))
}

pub fn byte_index_to_ascii_cursor(byte_index: usize) -> (usize, usize) {
    let line = byte_index / BYTES_PER_LINE;
    let byte_in_line = byte_index % BYTES_PER_LINE;
    (line, ASCII_START + byte_in_line)
}

/// Cursor after completing a hex nibble in overwrite mode.
pub fn next_hex_overwrite_cursor(byte_index: usize, nibble: usize, len: usize) -> (usize, usize) {
    if nibble == 0 {
        byte_index_to_hex_cursor(byte_index, 1)
    } else if byte_index + 1 < len {
        byte_index_to_hex_cursor(byte_index + 1, 0)
    } else {
        byte_index_to_hex_cursor(len, 0)
    }
}

/// Cursor after an ASCII edit in overwrite mode.
pub fn next_ascii_overwrite_cursor(byte_index: usize, len: usize) -> (usize, usize) {
    if byte_index + 1 < len {
        byte_index_to_ascii_cursor(byte_index + 1)
    } else {
        byte_index_to_hex_cursor(len, 0)
    }
}

/// Cursor after appending one byte at EOF.
pub fn cursor_after_append(len: usize) -> (usize, usize) {
    byte_index_to_hex_cursor(len, 0)
}

/// Cursor after deleting the byte at `index` from a buffer of `new_len` bytes.
pub fn cursor_after_delete(index: usize, new_len: usize) -> (usize, usize) {
    if new_len == 0 {
        (0, hex_col(0, 0))
    } else {
        byte_index_to_hex_cursor(index.min(new_len - 1), 0)
    }
}

pub fn parse_hex_pair(line: &str, byte_in_line: usize) -> Option<u8> {
    let hi = line.as_bytes().get(hex_col(byte_in_line, 0)).copied()?;
    let lo = line.as_bytes().get(hex_col(byte_in_line, 1)).copied()?;
    let pair = [hi, lo];
    u8::from_str_radix(std::str::from_utf8(&pair).ok()?, 16).ok()
}

/// Parse formatted hex-dump lines back into raw bytes.
pub fn rope_to_bytes(text: RopeSlice<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line_idx in 0..text.len_lines() {
        let line = text.line(line_idx).to_string();
        for byte_in_line in 0..BYTES_PER_LINE {
            match parse_hex_pair(&line, byte_in_line) {
                Some(byte) => bytes.push(byte),
                None => break,
            }
        }
    }
    bytes
}

/// Peek the start of a file to decide whether it should open as a hex dump.
pub fn file_looks_binary(path: &Path) -> io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 1024];
    let n = file.read(&mut header)?;
    Ok(is_binary(&header[..n]))
}

/// Map a virtual hex char index to a position relative to the hex viewport.
pub fn cursor_screen_pos(
    byte_len: usize,
    offset: ViewPosition,
    cursor: usize,
    viewport_height: usize,
    viewport_width: usize,
) -> Option<Position> {
    let max_char = display_len_chars(byte_len).saturating_sub(1);
    if cursor > max_char {
        return None;
    }
    let (line, col) = char_to_line_col(cursor, byte_len);

    let anchor = offset.anchor.min(max_char);
    let (first_line, _) = char_to_line_col(anchor, byte_len);
    let line_row = line.saturating_sub(first_line);
    if line_row < offset.vertical_offset {
        return None;
    }
    let screen_row = line_row - offset.vertical_offset;
    if screen_row >= viewport_height {
        return None;
    }

    let screen_col = col.checked_sub(offset.horizontal_offset)?;
    if screen_col >= viewport_width {
        return None;
    }

    Some(Position::new(screen_row, screen_col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_categories() {
        assert_eq!(byte_category(0), ByteCategory::Null);
        assert_eq!(byte_category(b'H'), ByteCategory::AsciiPrintable);
        assert_eq!(byte_category(b' '), ByteCategory::AsciiWhitespace);
        assert_eq!(byte_category(b'\n'), ByteCategory::AsciiWhitespace);
        assert_eq!(byte_category(0x7f), ByteCategory::AsciiOther);
        assert_eq!(byte_category(0x80), ByteCategory::NonAscii);
    }

    #[test]
    fn hexyl_colors() {
        assert_eq!(parse_hexyl_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_hexyl_color("bright blue"), Some(Color::LightBlue));
        assert_eq!(parse_hexyl_color("#ff0077"), Some(Color::Rgb(255, 0, 119)));
    }

    #[test]
    fn bytes_rope_roundtrip() {
        let cases: &[&[u8]] = &[
            b"",
            b"\x00",
            b"Hello",
            b"Hello\nWorld",
            &(0u8..32).collect::<Vec<_>>(),
        ];
        for bytes in cases {
            let rope = bytes_to_rope(bytes);
            assert_eq!(rope_to_bytes(rope.slice(..)), *bytes);
        }
    }

    #[test]
    fn partial_last_line_roundtrip() {
        let bytes = b"0123456789abcdef0"; // 17 bytes
        let rope = bytes_to_rope(bytes);
        assert_eq!(rope_to_bytes(rope.slice(..)), bytes);
    }

    #[test]
    fn edit_cursor_appends_at_eof() {
        assert_eq!(byte_slot(16, 17), ByteSlot::Existing(16));
        assert_eq!(byte_slot(17, 17), ByteSlot::Eof);
        assert_eq!(next_hex_overwrite_cursor(16, 1, 17), byte_index_to_hex_cursor(17, 0));
        assert_eq!(next_ascii_overwrite_cursor(16, 17), byte_index_to_hex_cursor(17, 0));
        assert_eq!(cursor_after_append(3), byte_index_to_hex_cursor(3, 0));
    }

    #[test]
    fn vertical_and_scroll_moves_lines() {
        use helix_core::movement::{Direction, Movement};
        use helix_core::Range;

        let bytes = b"0123456789abcdef"; // 16 bytes = 1 line
        let second_line = line_col_to_char(1, 0, bytes.len());
        let range = Range::point(0);
        let down = move_range(range, Direction::Forward, 1, Movement::Move, bytes.len(), true);
        assert_eq!(down.head, second_line);

        let offset = scroll_view(
            ViewPosition {
                anchor: 0,
                vertical_offset: 0,
                horizontal_offset: 0,
            },
            bytes.len(),
            1,
        );
        assert_eq!(offset.anchor, second_line);
    }

    #[test]
    fn virtual_addressing_matches_rope() {
        let bytes = b"Hello\x00World";
        let rope = bytes_to_rope(bytes);
        let virtual_len = display_len_chars(bytes.len());
        assert_eq!(virtual_len, rope.len_chars());
        for char_idx in 0..virtual_len {
            let (v_line, v_col) = char_to_line_col(char_idx, bytes.len());
            let r_line = rope.char_to_line(char_idx);
            let r_col = char_idx - rope.line_to_char(r_line);
            assert_eq!((v_line, v_col), (r_line, r_col), "char {char_idx}");
        }
    }

    #[test]
    fn ascii_region_padded_to_line_width() {
        let line = format_line(0, b"A");
        assert_eq!(line.len(), ASCII_START + BYTES_PER_LINE);
        assert_eq!(field_at_col(ASCII_START + 1), HexField::Ascii {
            byte_in_line: 1
        });
    }
}
