use helix_core::Selection;
use helix_view::{
    align_view, doc, doc_mut, editor::Action, theme::Style, view_mut, Align, DocumentId,
};
use tui::{
    text::{Span, Spans},
    widgets::Cell,
};

use crate::{
    commands::Context,
    compositor::Context as CompContext,
    ui::{overlay::overlaid, Picker, PickerColumn},
};

/// Fuzzy line picker scoped to the current buffer (`space+l`).
pub fn local_search(cx: &mut Context) {
    #[derive(Debug)]
    struct LineResult {
        line_num: usize,
        content: String,
    }

    struct LocalSearchConfig {
        number_style: Style,
    }

    let doc = doc!(cx.editor);
    let doc_id = doc.id();

    let lines: Vec<LineResult> = doc
        .text()
        .lines()
        .enumerate()
        .filter_map(|(line_num, line)| {
            let content = line.to_string();
            (!content.trim().is_empty()).then_some(LineResult { line_num, content })
        })
        .collect();

    let config = LocalSearchConfig {
        number_style: cx.editor.theme.get("constant.numeric.integer"),
    };

    let columns = [
        PickerColumn::new("line", |item: &LineResult, config: &LocalSearchConfig| {
            let line_num = (item.line_num + 1).to_string();
            let padding = " ".repeat(8_usize.saturating_sub(line_num.len()));
            Cell::from(Spans::from(vec![
                Span::styled(line_num, config.number_style),
                Span::raw(padding),
            ]))
        })
        .without_filtering(),
        PickerColumn::new("", |item: &LineResult, _| {
            Cell::from(Spans::from(vec![Span::raw(&item.content)]))
        }),
    ];

    let reg = cx.register.unwrap_or('/');
    cx.editor.registers.last_search_register = reg;

    let picker = Picker::new(
        columns,
        1,
        lines,
        config,
        move |cx, LineResult { line_num, .. }, action| {
            jump_to_line(cx, doc_id, *line_num, action);
        },
    )
    .with_preview(move |_editor, LineResult { line_num, .. }| {
        Some((doc_id.into(), Some((*line_num, *line_num))))
    })
    .with_history_register(Some(reg));

    cx.push_layer(Box::new(overlaid(picker)));
}

fn jump_to_line(
    cx: &mut CompContext<'_>,
    doc_id: DocumentId,
    line_num: usize,
    action: Action,
) {
    let view = view_mut!(cx.editor);
    let doc = doc_mut!(cx.editor, &doc_id);
    let text = doc.text();
    if line_num >= text.len_lines() {
        cx.editor.set_error(
            "The line you jumped to does not exist anymore because the file has changed.",
        );
        return;
    }
    let start = text.line_to_char(line_num);
    let end = text.line_to_char((line_num + 1).min(text.len_lines()));

    doc.set_selection(view.id, Selection::single(start, end));
    if action.align_view(view, doc.id()) {
        align_view(doc, view, Align::Center);
    }
}
