use helix_lsp::lsp::SymbolKind;
use helix_view::{
    document::SCRATCH_BUFFER_NAME,
    editor::BreadcrumbPathOptions::{File, Full},
    graphics::{Rect, Style},
    icons::ICONS,
    Document, Editor, View,
};
use tui::buffer::Buffer as Surface;

use super::symbol::display_symbol_kind;

pub fn render(
    editor: &Editor,
    doc: &Document,
    view: &View,
    viewport: Rect,
    surface: &mut Surface,
) {
    #[inline]
    #[must_use]
    fn draw_element(
        surface: &mut Surface,
        viewport: Rect,
        x: u16,
        content: &str,
        style: Style,
    ) -> u16 {
        let remaining = viewport.right().saturating_sub(x) as usize;
        surface
            .set_stringn(x, viewport.y, content, remaining, style)
            .0
    }

    fn truncate_name(name: &str, max_len: usize) -> std::borrow::Cow<'_, str> {
        const ELLIPSIS: char = '…';
        if max_len == 0 || name.chars().count() <= max_len {
            return std::borrow::Cow::Borrowed(name);
        }
        let chars: Vec<char> = name.chars().collect();
        let keep = max_len.saturating_sub(1);
        let head = keep.div_ceil(2).max(1);
        let tail = keep - head;
        let mut out = String::with_capacity(max_len * 4);
        out.extend(&chars[..head]);
        out.push(ELLIPSIS);
        if tail > 0 {
            out.extend(&chars[chars.len() - tail..]);
        }
        std::borrow::Cow::Owned(out)
    }

    let config = editor.config();

    let style = editor
        .theme
        .try_get_exact("ui.breadcrumb")
        .unwrap_or_else(|| editor.theme.get("ui.text"));

    surface.clear_with(viewport, style);

    let mut x = viewport.x.saturating_add(1);

    let separator = " > ";
    let separator_style = editor.theme.get("ui.breadcrumb.separator");
    let ellipsis_style = editor.theme.get("ui.breadcrumb.separator");
    let mut draw_separator = false;

    if matches!(config.breadcrumb.path, Full | File) {
        if let Some(path) = doc.relative_path() {
            let mut components = path.components().peekable();

            if matches!(config.breadcrumb.path, File) {
                while components.clone().nth(1).is_some() {
                    components.next();
                }
            }

            while let Some(component) = components.next() {
                if draw_separator {
                    x = draw_element(surface, viewport, x, separator, separator_style);
                } else {
                    draw_separator = true;
                }

                let segment = component.as_os_str().to_string_lossy();
                let is_directory = components.peek().is_some();

                let style = if is_directory {
                    editor.theme.get("ui.text.directory")
                } else {
                    style
                };

                x = draw_element(surface, viewport, x, &segment, style);
            }
        } else {
            x = draw_element(surface, viewport, x, SCRATCH_BUFFER_NAME, style);
        }
    }

    let icons = ICONS.load();
    let max_name_length = config.breadcrumb.max_name_length;

    if let Some(breadcrumb) = doc.breadcrumbs.get(&view.id) {
        if breadcrumb.elided() {
            if draw_separator {
                x = draw_element(surface, viewport, x, separator, separator_style);
            } else {
                draw_separator = true;
            }
            x = draw_element(surface, viewport, x, "…", ellipsis_style);
        }

        for symbol in breadcrumb.iter() {
            if draw_separator {
                x = draw_element(surface, viewport, x, separator, separator_style);
            } else {
                draw_separator = true;
            }

            let kind_name = display_symbol_kind(symbol.kind);

            if let Some(icon) = icons.kind().get(kind_name) {
                let icon_style = icon
                    .color()
                    .map(|color| Style::default().fg(color))
                    .unwrap_or_default();
                x = draw_element(surface, viewport, x, icon.glyph(), icon_style);
                x = draw_element(surface, viewport, x, " ", Style::default());
            }

            let style = symbol_style(editor, symbol.kind, style);
            let name = truncate_name(&symbol.name, max_name_length);
            x = draw_element(surface, viewport, x, &name, style);
        }
    }
}

fn symbol_style(editor: &Editor, kind: SymbolKind, default: Style) -> Style {
    match kind {
        SymbolKind::MODULE | SymbolKind::NAMESPACE | SymbolKind::PACKAGE => {
            editor.theme.get("namespace")
        }
        SymbolKind::OBJECT
        | SymbolKind::STRUCT
        | SymbolKind::INTERFACE
        | SymbolKind::CLASS => editor.theme.get("type"),
        SymbolKind::METHOD => editor.theme.get("function.method"),
        SymbolKind::FUNCTION => editor.theme.get("function"),
        SymbolKind::ENUM => editor.theme.get("type.enum"),
        SymbolKind::ENUM_MEMBER => editor.theme.get("type.enum.variant"),
        SymbolKind::FIELD | SymbolKind::PROPERTY => editor.theme.get("variable.other.member"),
        SymbolKind::VARIABLE => editor.theme.get("variable"),
        SymbolKind::CONSTANT => editor.theme.get("constant"),
        SymbolKind::CONSTRUCTOR => editor.theme.get("constructor"),
        SymbolKind::STRING => editor.theme.get("string"),
        SymbolKind::NUMBER => editor.theme.get("constant.numeric"),
        SymbolKind::BOOLEAN => editor.theme.get("constant.builtin.boolean"),
        SymbolKind::ARRAY => editor.theme.get("punctuation.bracket"),
        SymbolKind::KEY => editor.theme.get("label"),
        SymbolKind::NULL => editor.theme.get("constant.builtin"),
        SymbolKind::TYPE_PARAMETER => editor.theme.get("type.parameter"),
        _ => default,
    }
}
