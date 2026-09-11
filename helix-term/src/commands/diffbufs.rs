use std::cell::Cell;
use std::collections::HashSet;

use anyhow::bail;
use helix_core::{char_idx_at_visual_offset, visual_offset_from_anchor, Rope, RopeSlice};
use helix_loader::workspace_trust::TrustQuery;
use helix_vcs::DiffHandle;
use helix_view::view::ViewPosition;
use helix_view::{Document, DocumentId, Editor, View, ViewId};

thread_local! {
    static SYNCING_SCROLL: Cell<bool> = const { Cell::new(false) };
}

pub fn group_members(editor: &Editor, anchor: DocumentId) -> Vec<DocumentId> {
    editor
        .documents()
        .filter(|doc| doc.diff_group() == Some(anchor))
        .map(|doc| doc.id())
        .collect()
}

pub fn link_diff_group(editor: &mut Editor, doc_ids: &[DocumentId]) {
    if doc_ids.len() < 2 {
        return;
    }

    for &id in doc_ids {
        if editor.document(id).is_some_and(|doc| doc.in_diff_group()) {
            unlink_diff_group(editor, id);
        }
    }

    let anchor = doc_ids[0];
    let compare = doc_ids[1];
    let anchor_text = editor.document(anchor).map(|doc| doc.text().clone());
    let compare_text = editor.document(compare).map(|doc| doc.text().clone());

    for &id in doc_ids {
        let Some(doc) = editor.document_mut(id) else {
            continue;
        };
        doc.set_diff_group(Some(anchor));
        if id == anchor {
            doc.set_diff_compare(Some(compare));
            if let Some(text) = compare_text.clone() {
                doc.set_diff_base_rope(text);
            }
        } else {
            doc.set_diff_compare(None);
            if let Some(text) = anchor_text.clone() {
                doc.set_diff_base_rope(text);
            }
        }
    }
}

pub fn sync_group_on_change(editor: &mut Editor, changed: DocumentId, text: Rope) {
    let Some(anchor) = editor.document(changed).and_then(|doc| doc.diff_group()) else {
        return;
    };

    if changed == anchor {
        let members: Vec<DocumentId> = editor
            .documents()
            .filter(|doc| doc.diff_group() == Some(anchor) && doc.id() != anchor)
            .map(|doc| doc.id())
            .collect();
        for member in members {
            if let Some(doc) = editor.document_mut(member) {
                doc.set_diff_base_rope(text.clone());
            }
        }
        return;
    }

    if editor.document(anchor).and_then(|doc| doc.diff_compare()) == Some(changed) {
        if let Some(doc) = editor.document_mut(anchor) {
            doc.set_diff_base_rope(text);
        }
    }
}

pub fn restore_git_diff(editor: &mut Editor, doc_id: DocumentId) {
    let (path, workspace_root) = match editor.document(doc_id) {
        Some(doc) => (
            doc.path().map(|p| p.to_path_buf()),
            doc.workspace_root().to_path_buf(),
        ),
        None => return,
    };
    let Some(path) = path else {
        if let Some(doc) = editor.document_mut(doc_id) {
            doc.clear_diff();
        }
        return;
    };
    let trust_full = editor
        .workspace_trust
        .query(&workspace_root, TrustQuery::Git)
        .is_trusted();
    let diff_base = editor.diff_providers.get_diff_base(&path, trust_full);
    if let Some(doc) = editor.document_mut(doc_id) {
        if let Some(base) = diff_base {
            doc.set_diff_base(base);
        } else {
            doc.clear_diff();
        }
    }
}

pub fn unlink_diff_group(editor: &mut Editor, doc_id: DocumentId) {
    let Some(anchor) = editor.document(doc_id).and_then(|doc| doc.diff_group()) else {
        return;
    };
    for member in group_members(editor, anchor) {
        if let Some(doc) = editor.document_mut(member) {
            doc.set_diff_group(None);
            doc.set_diff_compare(None);
            doc.clear_diff();
        }
        restore_git_diff(editor, member);
    }
}

pub fn visible_buffer_ids(editor: &Editor) -> Vec<DocumentId> {
    let mut seen = HashSet::new();
    editor
        .tree
        .views()
        .map(|(view, _)| view.doc)
        .filter(|id| seen.insert(*id))
        .collect()
}

pub fn link_visible_buffers(editor: &mut Editor) -> anyhow::Result<()> {
    let doc_ids = visible_buffer_ids(editor);
    link_buffers(editor, &doc_ids, "buffers")
}

pub fn link_opened_buffers(editor: &mut Editor, doc_ids: &[DocumentId]) -> anyhow::Result<()> {
    link_buffers(editor, doc_ids, "files")
}

fn link_buffers(
    editor: &mut Editor,
    doc_ids: &[DocumentId],
    label: &str,
) -> anyhow::Result<()> {
    if doc_ids.len() < 2 {
        bail!("need at least 2 {label} to diff");
    }
    link_diff_group(editor, doc_ids);
    sync_diff_scroll(editor, editor.tree.focus);
    Ok(())
}

/// Keep diff-linked panes aligned by mapping the cursor line through diff hunks.
pub fn sync_diff_scroll(editor: &mut Editor, source_view_id: ViewId) {
    if SYNCING_SCROLL.get() {
        return;
    }

    let source_doc_id = editor.tree.get(source_view_id).doc;
    let Some(anchor) = editor
        .document(source_doc_id)
        .and_then(|doc| doc.diff_group())
    else {
        return;
    };

    let (source_line, col_in_line, viewport_row, horizontal_offset) = {
        let Some(doc) = editor.document(source_doc_id) else {
            return;
        };
        let view = editor.tree.get(source_view_id);
        let text = doc.text().slice(..);
        let cursor = doc.selection(source_view_id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_start = text.line_to_char(line);
        let offset = doc.view_offset(source_view_id);
        (
            line as u32,
            cursor.saturating_sub(line_start),
            cursor_viewport_row(view, doc, text, cursor),
            offset.horizontal_offset,
        )
    };

    let anchor_line = map_line_to_anchor(editor, anchor, source_doc_id, source_line);

    let targets: Vec<(ViewId, DocumentId)> = editor
        .tree
        .views()
        .filter(|(view, _)| view.id != source_view_id)
        .map(|(view, _)| (view.id, view.doc))
        .collect();

    let mut updates = Vec::new();
    for (view_id, doc_id) in targets {
        let in_group = editor
            .document(doc_id)
            .and_then(|doc| doc.diff_group())
            == Some(anchor);
        if !in_group {
            continue;
        }
        let target_line = map_line_from_anchor(editor, anchor, doc_id, anchor_line);
        let Some(doc) = editor.document(doc_id) else {
            continue;
        };
        let view = editor.tree.get(view_id);
        let target_char = char_at_line_col(doc.text().slice(..), target_line, col_in_line);
        updates.push((
            view_id,
            doc_id,
            view_position_for_char_at_row(
                view,
                doc,
                target_char,
                viewport_row,
                horizontal_offset,
            ),
        ));
    }

    SYNCING_SCROLL.set(true);
    for (view_id, doc_id, position) in updates {
        if let Some(doc) = editor.document_mut(doc_id) {
            doc.set_view_offset(view_id, position);
        }
    }
    SYNCING_SCROLL.set(false);
}

/// Visual row of `cursor` within the view viewport (0 = top line on screen).
fn cursor_viewport_row(view: &View, doc: &Document, text: RopeSlice<'_>, cursor: usize) -> usize {
    if let Some(pos) = view.screen_coords_at_pos(doc, text, cursor) {
        return pos.row;
    }

    let view_offset = doc.view_offset(view.id);
    let viewport = view.inner_area(doc);
    let text_fmt = doc.text_format(viewport.width, None);
    let annotations = view.text_annotations(doc, None);
    visual_offset_from_anchor(
        text,
        view_offset.anchor,
        cursor,
        &text_fmt,
        &annotations,
        view_offset.vertical_offset + viewport.height as usize,
    )
    .ok()
    .map(|(pos, _)| pos.row.saturating_sub(view_offset.vertical_offset))
    .unwrap_or(0)
}

fn char_at_line_col(text: RopeSlice<'_>, line: u32, col_in_line: usize) -> usize {
    if text.len_lines() == 0 {
        return 0;
    }
    let line = (line as usize).min(text.len_lines().saturating_sub(1));
    let line_start = text.line_to_char(line);
    let line_end = text.line_to_char(line + 1).min(text.len_chars());
    let line_len = line_end.saturating_sub(line_start);
    line_start + col_in_line.min(line_len.saturating_sub(1).max(0))
}

fn view_position_for_char_at_row(
    view: &View,
    doc: &Document,
    char_idx: usize,
    viewport_row: usize,
    horizontal_offset: usize,
) -> ViewPosition {
    let text = doc.text().slice(..);
    if text.len_chars() == 0 {
        return ViewPosition {
            anchor: 0,
            vertical_offset: 0,
            horizontal_offset: 0,
        };
    }
    let char_idx = char_idx.min(text.len_chars().saturating_sub(1));
    let viewport = view.inner_area(doc);
    let text_fmt = doc.text_format(viewport.width, None);
    let annotations = view.text_annotations(doc, None);
    let (anchor, vertical_offset) = char_idx_at_visual_offset(
        text,
        char_idx,
        -(viewport_row as isize),
        0,
        &text_fmt,
        &annotations,
    );
    ViewPosition {
        anchor,
        vertical_offset,
        horizontal_offset,
    }
}

fn map_line_to_anchor(
    editor: &Editor,
    anchor: DocumentId,
    doc_id: DocumentId,
    line: u32,
) -> u32 {
    if doc_id == anchor {
        return line;
    }
    let Some(handle) = editor
        .document(doc_id)
        .and_then(|doc| doc.diff_handle().cloned())
    else {
        return line;
    };
    map_doc_line_to_base(&handle, line)
}

fn map_line_from_anchor(
    editor: &Editor,
    anchor: DocumentId,
    doc_id: DocumentId,
    line: u32,
) -> u32 {
    if doc_id == anchor {
        return line;
    }
    let Some(handle) = editor
        .document(doc_id)
        .and_then(|doc| doc.diff_handle().cloned())
    else {
        return line;
    };
    map_base_line_to_doc(&handle, line)
}

fn map_doc_line_to_base(handle: &DiffHandle, doc_line: u32) -> u32 {
    let diff = handle.load();
    if diff.is_empty() {
        return doc_line;
    }

    for i in 0..diff.len() {
        let hunk = diff.nth_hunk(i);
        if doc_line < hunk.after.start {
            let (before_end, after_end) = previous_hunk_ends(handle, i);
            return before_end + doc_line.saturating_sub(after_end);
        }
        if doc_line < hunk.after.end {
            return map_line_within_hunk_to_base(&hunk, doc_line);
        }
    }

    let last = diff.nth_hunk(diff.len() - 1);
    last.before.end + doc_line.saturating_sub(last.after.end)
}

fn map_base_line_to_doc(handle: &DiffHandle, base_line: u32) -> u32 {
    let diff = handle.load();
    if diff.is_empty() {
        return base_line;
    }

    for i in 0..diff.len() {
        let hunk = diff.nth_hunk(i);
        if base_line < hunk.before.start {
            let (before_end, after_end) = previous_hunk_ends(handle, i);
            return after_end + base_line.saturating_sub(before_end);
        }
        if base_line < hunk.before.end {
            return map_line_within_hunk_to_doc(&hunk, base_line);
        }
    }

    let last = diff.nth_hunk(diff.len() - 1);
    last.after.end + base_line.saturating_sub(last.before.end)
}

fn previous_hunk_ends(handle: &DiffHandle, index: u32) -> (u32, u32) {
    if index == 0 {
        (0, 0)
    } else {
        let diff = handle.load();
        let prev = diff.nth_hunk(index - 1);
        (prev.before.end, prev.after.end)
    }
}

fn map_line_within_hunk_to_base(hunk: &helix_vcs::Hunk, doc_line: u32) -> u32 {
    if hunk.is_pure_insertion() {
        return hunk.before.start;
    }
    if hunk.is_pure_removal() {
        return hunk.before.start;
    }
    let offset = doc_line.saturating_sub(hunk.after.start);
    let paired = (hunk.before.end - hunk.before.start).min(hunk.after.end - hunk.after.start);
    if offset < paired {
        hunk.before.start + offset
    } else {
        hunk.before.end.saturating_sub(1)
    }
}

fn map_line_within_hunk_to_doc(hunk: &helix_vcs::Hunk, base_line: u32) -> u32 {
    if hunk.is_pure_removal() {
        return hunk.after.start;
    }
    if hunk.is_pure_insertion() {
        return hunk.after.start;
    }
    let offset = base_line.saturating_sub(hunk.before.start);
    let paired = (hunk.before.end - hunk.before.start).min(hunk.after.end - hunk.after.start);
    if offset < paired {
        hunk.after.start + offset
    } else {
        hunk.after.end.saturating_sub(1)
    }
}
