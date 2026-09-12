use helix_view::current;
use helix_view::document::DocumentOpenError;

use crate::compositor;

pub fn toggle(cx: &mut compositor::Context) -> anyhow::Result<()> {
    let (view, doc) = current!(cx.editor);
    let view_id = view.id;
    let doc_id = doc.id();

    if doc.is_hex_dump() {
        let loader = cx.editor.syn_loader.load();
        if let Err(err) = cx.editor.document_mut(doc_id).unwrap().exit_hex_dump(view_id, &loader)
        {
            match err {
                DocumentOpenError::IoError(err) => anyhow::bail!(err),
                DocumentOpenError::IrregularFile => {
                    anyhow::bail!("can't leave hex view without a file path")
                }
            }
        }
    } else {
        cx.editor
            .document_mut(doc_id)
            .unwrap()
            .enter_hex_dump(view_id)?;
    }

    Ok(())
}
