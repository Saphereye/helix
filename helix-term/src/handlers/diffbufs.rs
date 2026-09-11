use helix_event::register_hook;
use helix_view::events::{DocumentDidChange, DocumentDidClose};

use crate::commands::diffbufs;
use crate::job;

pub fn register_hooks() {
    register_hook!(move |event: &mut DocumentDidChange<'_>| {
        if event.ghost_transaction {
            return Ok(());
        }
        if !event.doc.in_diff_group() {
            return Ok(());
        }
        let doc_id = event.doc.id();
        let text = event.doc.text().clone();
        job::dispatch_blocking(move |editor, _| {
            diffbufs::sync_group_on_change(editor, doc_id, text);
        });
        Ok(())
    });

    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        if event.doc.in_diff_group() {
            diffbufs::unlink_diff_group(event.editor, event.doc.id());
        }
        Ok(())
    });
}
