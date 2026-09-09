use std::{collections::HashMap, time::Duration};

use helix_core::{diff::compare_ropes, Rope};
use helix_event::{register_hook, send_blocking, AsyncHook};
use helix_view::{
    events::{DocumentDidChange, DocumentDidClose},
    DocumentId, Editor,
};
use tokio::{sync::mpsc, time::Instant};

use crate::job;

// TODO should be configurable?
const SYNTAX_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Default)]
struct SyntaxHandler {
    pending: HashMap<DocumentId, Pending>,
}

struct Pending {
    old_text: Rope,
    text: Rope,
}

enum SyntaxEvent {
    Change {
        doc: DocumentId,
        old_text: Rope,
        text: Rope,
    },
    Close(DocumentId),
}

impl AsyncHook for SyntaxHandler {
    type Event = SyntaxEvent;

    fn handle_event(&mut self, event: Self::Event, timeout: Option<Instant>) -> Option<Instant> {
        match event {
            SyntaxEvent::Change {
                doc,
                old_text,
                text,
            } => {
                match self.pending.get_mut(&doc) {
                    Some(pending) => pending.text = text,
                    None => {
                        self.pending.insert(doc, Pending { old_text, text });
                    }
                }
                Some(Instant::now() + SYNTAX_DEBOUNCE)
            }
            SyntaxEvent::Close(doc) => {
                self.pending.remove(&doc);
                timeout
            }
        }
    }

    fn finish_debounce(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        job::dispatch_blocking(move |editor, _| update_syntax(editor, pending));
    }
}

fn update_syntax(editor: &mut Editor, pending: HashMap<DocumentId, Pending>) {
    let loader = editor.syn_loader.load();
    for (doc_id, update) in pending {
        let Some(doc) = editor.document_mut(doc_id) else {
            continue;
        };
        let Some(syntax) = doc.syntax.as_mut() else {
            continue;
        };
        let diff = compare_ropes(&update.old_text, &update.text);
        if let Err(err) = syntax.update(
            update.old_text.slice(..),
            update.text.slice(..),
            diff.changes(),
            &loader,
        ) {
            log::error!("tree-sitter parser failed, disabling syntax highlighting: {err}");
            doc.syntax = None;
        }
    }
    helix_event::request_redraw();
}

pub fn spawn() {
    let tx = SyntaxHandler::default().spawn();
    register_hooks(&tx);
}

fn register_hooks(tx: &mpsc::Sender<SyntaxEvent>) {
    let change_tx = tx.clone();
    register_hook!(move |event: &mut DocumentDidChange<'_>| {
        if event.ghost_transaction || event.doc.syntax().is_none() {
            return Ok(());
        }
        send_blocking(
            &change_tx,
            SyntaxEvent::Change {
                doc: event.doc.id(),
                old_text: event.old_text.clone(),
                text: event.doc.text().clone(),
            },
        );
        Ok(())
    });

    let close_tx = tx.clone();
    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        send_blocking(&close_tx, SyntaxEvent::Close(event.doc.id()));
        Ok(())
    });
}
