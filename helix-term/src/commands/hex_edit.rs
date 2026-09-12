use helix_core::{movement::Movement, Selection};
use helix_view::{current, current_ref, doc_mut, hex_dump, hex_dump::HexField, DocumentId, ViewId};
use helix_core::movement::Direction;

use crate::commands::Context;

/// Handle cursor movement in hex mode. Returns `true` when the move was handled.
pub fn intercept_move(
    cx: &mut Context,
    vertical: bool,
    dir: Direction,
    count: usize,
    behaviour: Movement,
) -> bool {
    let (view, doc) = current!(cx.editor);
    if !doc.is_hex_dump() {
        return false;
    }
    let byte_len = doc.hex_bytes().map_or(0, |b| b.len());
    let selection = doc.selection(view.id).clone().transform(|range| {
        hex_dump::move_range(range, dir, count, behaviour, byte_len, vertical)
    });
    doc.set_selection(view.id, selection);
    true
}

pub const HEX_EDIT_ONLY_MSG: &str = "hex edit: use i/r in hex or ASCII columns";

/// Block rope edits that would desync from the byte buffer.
pub fn block_plaintext_edit(cx: &mut Context) -> bool {
    let (_, doc) = current_ref!(cx.editor);
    if doc.is_hex_dump() {
        cx.editor.set_status(HEX_EDIT_ONLY_MSG);
        true
    } else {
        false
    }
}

pub fn replace_key(cx: &mut Context, ch: char) -> bool {
    apply_hex_char(cx, ch)
}

pub fn insert_char(cx: &mut Context, c: char) -> bool {
    apply_hex_char(cx, c)
}

pub fn delete_byte_forward(cx: &mut Context) -> bool {
    delete_byte(cx, true)
}

pub fn delete_byte_backward(cx: &mut Context) -> bool {
    delete_byte(cx, false)
}

/// Normal-mode `x`: delete the byte under the cursor.
pub fn delete_byte_normal(cx: &mut Context) -> bool {
    delete_byte(cx, true)
}

struct HexCursor {
    raw_index: usize,
    hex_len: usize,
    field: HexField,
}

fn hex_cursor(cx: &Context) -> Option<(ViewId, DocumentId, HexCursor)> {
    let (view, doc) = current_ref!(cx.editor);
    if !doc.is_hex_dump() {
        return None;
    }
    let bytes = doc.hex_bytes()?;
    let byte_len = bytes.len();
    let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
    let (line, col) = hex_dump::char_to_line_col(cursor, byte_len);
    let field = hex_dump::field_at_col(col);
    let raw_index = match field {
        HexField::Hex { byte_in_line, .. } | HexField::Ascii { byte_in_line } => {
            hex_dump::raw_byte_index(line, byte_in_line)
        }
        _ => return None,
    };
    Some((
        view.id,
        doc.id(),
        HexCursor {
            raw_index,
            hex_len: doc.hex_bytes().map_or(0, |b| b.len()),
            field,
        },
    ))
}

/// In-place overwrite; appends one byte when cursor is past EOF.
fn apply_hex_char(cx: &mut Context, c: char) -> bool {
    let Some((view_id, doc_id, pos)) = hex_cursor(cx) else {
        return false;
    };

    let slot = hex_dump::byte_slot(pos.raw_index, pos.hex_len);

    match pos.field {
        HexField::Hex { nibble, .. } => {
            let Some(digit) = c.to_digit(16) else {
                cx.editor.set_status("expected hex digit");
                return true;
            };

            if slot == hex_dump::ByteSlot::Eof && nibble != 0 {
                cx.editor.set_status("expected hex digit at high nibble");
                return true;
            }

            let (next_line, next_col) = match (slot, nibble) {
                (hex_dump::ByteSlot::Existing(idx), 0) => {
                    let old = cx.editor.documents[&doc_id].hex_bytes().unwrap()[idx];
                    let new_byte = (digit as u8) << 4 | (old & 0x0f);
                    doc_mut!(cx.editor, &doc_id).set_hex_byte(view_id, idx, new_byte);
                    hex_dump::next_hex_overwrite_cursor(idx, 0, pos.hex_len)
                }
                (hex_dump::ByteSlot::Existing(idx), 1) => {
                    let old = cx.editor.documents[&doc_id].hex_bytes().unwrap()[idx];
                    let new_byte = (old & 0xf0) | digit as u8;
                    doc_mut!(cx.editor, &doc_id).set_hex_byte(view_id, idx, new_byte);
                    hex_dump::next_hex_overwrite_cursor(idx, 1, pos.hex_len)
                }
                (hex_dump::ByteSlot::Eof, 0) => {
                    doc_mut!(cx.editor, &doc_id)
                        .append_hex_byte(view_id, (digit as u8) << 4);
                    let idx = cx.editor.documents[&doc_id]
                        .hex_bytes()
                        .map_or(0, |b| b.len())
                        - 1;
                    hex_dump::byte_index_to_hex_cursor(idx, 1)
                }
                _ => unreachable!(),
            };

            set_cursor_on_line(cx, next_line, next_col);
            true
        }
        HexField::Ascii { .. } => {
            if !c.is_ascii() {
                cx.editor.set_status("expected ASCII character");
                return true;
            }

            let (next_line, next_col) = match slot {
                hex_dump::ByteSlot::Existing(idx) => {
                    doc_mut!(cx.editor, &doc_id).set_hex_byte(view_id, idx, c as u8);
                    hex_dump::next_ascii_overwrite_cursor(idx, pos.hex_len)
                }
                hex_dump::ByteSlot::Eof => {
                    doc_mut!(cx.editor, &doc_id).append_hex_byte(view_id, c as u8);
                    hex_dump::cursor_after_append(pos.hex_len + 1)
                }
            };

            set_cursor_on_line(cx, next_line, next_col);
            true
        }
        _ => {
            cx.editor.set_status("move cursor to hex or ASCII column");
            true
        }
    }
}

fn delete_byte(cx: &mut Context, forward: bool) -> bool {
    let Some((view_id, doc_id, pos)) = hex_cursor(cx) else {
        return false;
    };

    if pos.hex_len == 0 {
        cx.editor.set_status("empty buffer");
        return true;
    }

    let delete_index = match (hex_dump::byte_slot(pos.raw_index, pos.hex_len), forward) {
        (hex_dump::ByteSlot::Eof, _) => pos.hex_len - 1,
        (hex_dump::ByteSlot::Existing(idx), true) => idx,
        (hex_dump::ByteSlot::Existing(idx), false) => idx.saturating_sub(1),
    };

    doc_mut!(cx.editor, &doc_id).remove_hex_byte(view_id, delete_index);
    let new_len = cx.editor.documents[&doc_id]
        .hex_bytes()
        .map_or(0, |b| b.len());
    let (line, col) = hex_dump::cursor_after_delete(delete_index, new_len);
    set_cursor_on_line(cx, line, col);
    true
}

fn set_cursor_on_line(cx: &mut Context, line: usize, col: usize) {
    let (view, doc) = current!(cx.editor);
    let scrolloff = doc.config.load().scrolloff as usize;
    let byte_len = doc.hex_bytes().map_or(0, |b| b.len());
    let char_idx = hex_dump::line_col_to_char(line, col, byte_len);
    doc.set_selection(view.id, Selection::point(char_idx));
    view.ensure_cursor_in_view(doc, scrolloff);
    helix_event::request_redraw();
}
