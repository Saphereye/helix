use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use helix_event::register_hook;
use helix_loader::workspace_trust::TrustQuery;
use helix_view::events::{DocumentDidClose, DocumentDidOpen};
use helix_view::{DocumentId, Editor};
use once_cell::sync::OnceCell;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::job;
use crate::ui::{menu::Item, PromptEvent, Select};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const FILE_WATCHER_ID: &str = "file-watcher-reload";

static CMD_TX: OnceCell<UnboundedSender<WatcherCommand>> = OnceCell::new();
static IGNORED: OnceCell<Mutex<HashMap<DocumentId, SystemTime>>> = OnceCell::new();

enum WatcherCommand {
    Watch(DocumentId),
    Unwatch(DocumentId),
}

#[derive(Clone, Copy)]
enum ReloadChoice {
    Reload,
    Ignore,
}

impl Item for ReloadChoice {
    type Data = ();

    fn format(&self, _data: &Self::Data) -> tui::widgets::Row<'_> {
        match self {
            ReloadChoice::Reload => "Reload",
            ReloadChoice::Ignore => "Ignore",
        }
        .into()
    }
}

pub fn spawn() {
    let (cmd_tx, mut cmd_rx) = unbounded_channel::<WatcherCommand>();
    let _ = CMD_TX.set(cmd_tx);
    let _ = IGNORED.set(Mutex::new(HashMap::new()));

    tokio::spawn(async move {
        let mut watched = HashSet::new();
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    match cmd {
                        WatcherCommand::Watch(doc_id) => {
                            watched.insert(doc_id);
                        }
                        WatcherCommand::Unwatch(doc_id) => {
                            watched.remove(&doc_id);
                            if let Ok(mut ignored) = IGNORED.get().unwrap().lock() {
                                ignored.remove(&doc_id);
                            }
                        }
                    }
                }
                _ = interval.tick() => {
                    if watched.is_empty() {
                        continue;
                    }
                    let docs: Vec<DocumentId> = watched.iter().copied().collect();
                    job::dispatch_blocking(move |editor, compositor| {
                        for doc_id in docs {
                            check_document(editor, compositor, doc_id);
                        }
                    });
                }
            }
        }
    });
}

pub fn register_hooks() {
    register_hook!(move |event: &mut DocumentDidOpen<'_>| {
        if !event.editor.config().auto_reload {
            return Ok(());
        }
        let doc_id = event.doc;
        let has_file = event
            .editor
            .document(doc_id)
            .and_then(|doc| doc.path())
            .is_some_and(|path| path.is_file());
        if has_file {
            watch(doc_id);
        }
        Ok(())
    });

    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        unwatch(event.doc.id());
        Ok(())
    });
}

fn watch(doc_id: DocumentId) {
    if let Some(tx) = CMD_TX.get() {
        let _ = tx.send(WatcherCommand::Watch(doc_id));
    }
}

fn unwatch(doc_id: DocumentId) {
    if let Some(tx) = CMD_TX.get() {
        let _ = tx.send(WatcherCommand::Unwatch(doc_id));
    }
}

fn check_document(
    editor: &mut Editor,
    compositor: &mut crate::compositor::Compositor,
    doc_id: DocumentId,
) {
    if !editor.config().auto_reload {
        return;
    }
    let Some(doc) = editor.document(doc_id) else {
        return;
    };
    if doc.path().is_none() || !doc.changed_on_disk() {
        return;
    }

    let Some(mtime) = doc.disk_mtime() else {
        return;
    };
    if IGNORED
        .get()
        .and_then(|ignored| ignored.lock().ok())
        .is_some_and(|ignored| ignored.get(&doc_id) == Some(&mtime))
    {
        return;
    }

    if doc.is_modified() {
        compositor.replace_or_push(FILE_WATCHER_ID, select_reload(doc_id, mtime));
        return;
    }

    reload_document(editor, doc_id);
    editor.set_status(format!(
        "Reloaded {}",
        editor
            .document(doc_id)
            .map(|doc| doc.display_name().into_owned())
            .unwrap_or_default()
    ));
}

fn select_reload(doc_id: DocumentId, mtime: SystemTime) -> Select<ReloadChoice> {
    Select::new(
        "File changed on disk",
        [ReloadChoice::Reload, ReloadChoice::Ignore],
        (),
        move |editor, choice, event| {
            if event != PromptEvent::Validate {
                return;
            }
            match choice {
                ReloadChoice::Reload => reload_document(editor, doc_id),
                ReloadChoice::Ignore => {
                    if let Some(ignored) = IGNORED.get() {
                        if let Ok(mut map) = ignored.lock() {
                            map.insert(doc_id, mtime);
                        }
                    }
                }
            }
        },
    )
}

fn reload_document(editor: &mut Editor, doc_id: DocumentId) {
    let trust_full = {
        let doc = doc!(editor, &doc_id);
        editor
            .workspace_trust
            .query(doc.workspace_root(), TrustQuery::Git)
            .is_trusted()
    };
    let path = editor
        .document(doc_id)
        .and_then(|doc| doc.path().map(std::path::Path::to_path_buf));
    let scrolloff = editor.config().scrolloff;

    let view_ids: Vec<_> = editor
        .tree
        .views()
        .filter_map(|(view, _)| (view.doc == doc_id).then_some(view.id))
        .collect();
    let Some(&view_id) = view_ids.first() else {
        return;
    };

    let view = view_mut!(editor, view_id);
    let doc = doc_mut!(editor, &doc_id);
    if doc.reload(view, &editor.diff_providers, trust_full).is_err() {
        return;
    }

    for &vid in &view_ids {
        let view = view_mut!(editor, vid);
        view.sync_changes(doc);
        view.ensure_cursor_in_view(doc, scrolloff);
    }

    if let Some(path) = path {
        editor
            .language_servers
            .file_event_handler
            .file_changed(path);
    }

    if let Some(ignored) = IGNORED.get() {
        if let Ok(mut map) = ignored.lock() {
            map.remove(&doc_id);
        }
    }
}
