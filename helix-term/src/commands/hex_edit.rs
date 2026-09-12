use helix_core::Selection;
use helix_view::{current, current_ref, doc_mut, hex_dump, hex_dump::HexField};

use crate::commands::Context;

pub fn replace_key(cx: &mut Context, ch: char) -> bool {
    apply_hex_char(cx, ch)
}

pub fn insert_char(cx: &mut Context, c: char) -> bool {
    apply_hex_char(cx, c)
}

fn apply_hex_char(cx: &mut Context, c: char) -> bool {
    let (view, doc) = current_ref!(cx.editor);
    if !doc.is_hex_dump() {
        return false;
    }

    let view_id = view.id;
    let doc_id = doc.id();
    let text = doc.text();
    let range = doc.selection(view_id).primary();
    let cursor = range.cursor(text.slice(..));
    let line = text.char_to_line(cursor);
    let col = cursor - text.line_to_char(line);
    let line_str = text.line(line).to_string();
    let line_len = line_str.len();
    let hex_len = doc.hex_bytes().map_or(0, |b| b.len());

    match hex_dump::field_at_col(line_len, col) {
        HexField::Hex {
            byte_in_line,
            nibble,
        } => {
            let Some(digit) = c.to_digit(16) else {
                cx.editor.set_status("expected hex digit");
                return true;
            };
            let byte_index = line * hex_dump::BYTES_PER_LINE + byte_in_line;
            if byte_index >= hex_len {
                return true;
            }

            let old = doc.hex_bytes().unwrap()[byte_index];
            let new_byte = if nibble == 0 {
                (digit as u8) << 4 | (old & 0x0f)
            } else {
                (old & 0xf0) | digit as u8
            };

            let next_col = if nibble == 0 {
                hex_dump::hex_col(byte_in_line, 1)
            } else if byte_in_line + 1 < hex_dump::BYTES_PER_LINE && byte_index + 1 < hex_len {
                hex_dump::hex_col(byte_in_line + 1, 0)
            } else {
                hex_dump::hex_col(byte_in_line, 1)
            };

            doc_mut!(cx.editor, &doc_id).set_hex_byte(view_id, byte_index, new_byte);
            set_cursor_on_line(cx, line, next_col);
            true
        }
        HexField::Ascii { byte_in_line } => {
            if !c.is_ascii() {
                cx.editor.set_status("expected ASCII character");
                return true;
            }
            let byte_index = line * hex_dump::BYTES_PER_LINE + byte_in_line;
            if byte_index >= hex_len {
                return true;
            }

            let next_col = if byte_in_line + 1 < hex_dump::BYTES_PER_LINE && byte_index + 1 < hex_len
            {
                hex_dump::ASCII_START + byte_in_line + 1
            } else {
                col
            };

            doc_mut!(cx.editor, &doc_id).set_hex_byte(view_id, byte_index, c as u8);
            set_cursor_on_line(cx, line, next_col);
            true
        }
        _ => {
            cx.editor.set_status("move cursor to hex or ASCII column");
            true
        }
    }
}

fn set_cursor_on_line(cx: &mut Context, line: usize, col: usize) {
    let (view, doc) = current!(cx.editor);
    let char_idx = doc.text().line_to_char(line) + col;
    doc.set_selection(view.id, Selection::point(char_idx));
    helix_event::request_redraw();
}
