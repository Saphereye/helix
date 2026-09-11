use std::time::Duration;

use helix_event::{register_hook, send_blocking, AsyncHook};
use helix_view::events::{DocumentDidChange, DocumentDidClose, SelectionDidChange};
use helix_view::ViewId;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;

use crate::commands::diffbufs;
use crate::job;

/// Debounce cursor-driven scroll sync so holding `j`/`k` on huge logs does not
/// map lines on every key repeat. Explicit scroll (Ctrl-d/u) still syncs immediately.
const SCROLL_SYNC_DEBOUNCE: Duration = Duration::from_millis(16);

struct ScrollSyncHandler {
    pending: Option<ViewId>,
}

impl Default for ScrollSyncHandler {
    fn default() -> Self {
        Self { pending: None }
    }
}

impl AsyncHook for ScrollSyncHandler {
    type Event = ViewId;

    fn handle_event(&mut self, event: Self::Event, _timeout: Option<Instant>) -> Option<Instant> {
        self.pending = Some(event);
        Some(Instant::now() + SCROLL_SYNC_DEBOUNCE)
    }

    fn finish_debounce(&mut self) {
        let Some(view_id) = self.pending.take() else {
            return;
        };
        job::dispatch_blocking(move |editor, _| {
            diffbufs::sync_diff_scroll(editor, view_id);
        });
    }
}

pub fn spawn() {
    let tx = ScrollSyncHandler::default().spawn();
    register_hooks(&tx);
}

fn register_hooks(scroll_sync_tx: &Sender<ViewId>) {
    let scroll_sync_tx = scroll_sync_tx.clone();
    register_hook!(move |event: &mut SelectionDidChange<'_>| {
        if !event.doc.in_diff_group() {
            return Ok(());
        }
        send_blocking(&scroll_sync_tx, event.view);
        Ok(())
    });

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
