use std::collections::HashSet;

use anyhow::bail;
use helix_core::Rope;
use helix_loader::workspace_trust::TrustQuery;
use helix_view::{DocumentId, Editor};

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
    Ok(())
}
