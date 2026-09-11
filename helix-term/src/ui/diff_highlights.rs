use helix_core::{diff::word_diff_changed_ranges, syntax::OverlayHighlights, RopeSlice};
use helix_view::{Document, Theme};

pub fn inline_diff_highlights(
    doc: &Document,
    theme: &Theme,
    anchor: usize,
    height: u16,
) -> Option<OverlayHighlights> {
    let handle = doc.diff_handle()?;
    let diff = handle.load();
    if diff.is_empty() {
        return None;
    }

    let highlight = theme
        .find_highlight_exact("diff.delta")
        .or_else(|| theme.find_highlight_exact("diff.plus"))?;

    let text = doc.text().slice(..);
    let anchor = anchor.min(text.len_chars());
    let first_line = text.char_to_line(anchor);
    let last_line = (first_line + height as usize).min(text.len_lines().saturating_sub(1));

    let base = diff.diff_base().slice(..);
    let mut ranges = Vec::new();

    for hunk_i in 0..diff.len() {
        let hunk = diff.nth_hunk(hunk_i);
        if hunk.is_pure_insertion() || hunk.is_pure_removal() {
            continue;
        }

        let paired = (hunk.before.end - hunk.before.start).min(hunk.after.end - hunk.after.start);
        for offset in 0..paired {
            let doc_line = hunk.after.start + offset;
            if doc_line < first_line as u32 || doc_line > last_line as u32 {
                continue;
            }

            let base_line = hunk.before.start + offset;
            let Some(base_str) = line_string(base, base_line) else {
                continue;
            };
            let Some(doc_str) = line_string(text, doc_line) else {
                continue;
            };
            if base_str == doc_str {
                continue;
            }

            let line_char_start = text.line_to_char(doc_line as usize);
            for word_range in word_diff_changed_ranges(&base_str, &doc_str) {
                ranges.push(line_char_start + word_range.start..line_char_start + word_range.end);
            }
        }
    }

    if ranges.is_empty() {
        return None;
    }

    Some(OverlayHighlights::Homogeneous { highlight, ranges })
}

fn line_string(text: RopeSlice<'_>, line: u32) -> Option<String> {
    let line = line as usize;
    if line >= text.len_lines() {
        return None;
    }
    Some(text.line(line).to_string())
}
