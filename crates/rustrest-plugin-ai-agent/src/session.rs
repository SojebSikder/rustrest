//! Saved chat sessions: an index file listing them, plus one transcript file
//! per session, all in the plugin's storage.

use crate::chat::Transcript;
use crate::host;
use serde::{Deserialize, Serialize};

/// Saved sessions, most recently updated first. Each session's transcript
/// lives in its own `session-<id>.json` (see [`session_file`]).
const INDEX_FILE: &str = "sessions.json";

/// Saved sessions kept in history - the oldest are deleted beyond this.
pub(crate) const MAX_SAVED_SESSIONS: usize = 50;

fn session_file(id: u64) -> String {
    format!("session-{id}.json")
}

/// One entry in the history list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionMeta {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) message_count: usize,
    #[serde(default)]
    pub(crate) total_tokens: u64,
    #[serde(default)]
    pub(crate) cost_usd: Option<f64>,
}

/// A saved transcript, resumable from the history list.
#[derive(Debug, Serialize, Deserialize)]
struct SavedSession {
    id: u64,
    title: String,
    messages: Transcript,
}

/// The history list, kept in sync with the session files on disk.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct SessionHistory {
    next_id: u64,
    /// most recently updated first
    sessions: Vec<SessionMeta>,
}

impl SessionHistory {
    /// The saved history, or an empty one if there isn't a readable index.
    pub(crate) fn load() -> Self {
        host::read_json(INDEX_FILE).unwrap_or_default()
    }

    pub(crate) fn entries(&self) -> &[SessionMeta] {
        &self.sessions
    }

    /// A fresh id for a chat that's about to be saved for the first time.
    pub(crate) fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Writes `transcript` as session `id` and moves it to the top of the
    /// history, deleting the oldest sessions beyond [`MAX_SAVED_SESSIONS`].
    pub(crate) fn save(&mut self, id: u64, transcript: &Transcript) {
        let title = transcript.title();
        let session = SavedSession {
            id,
            title: title.clone(),
            messages: transcript.clone(),
        };
        if host::write_json(&session_file(id), &session).is_err() {
            return;
        }

        let (usage, cost_usd) = transcript.totals();
        let dropped = self.record(SessionMeta {
            id,
            title,
            message_count: transcript.len(),
            total_tokens: usage.total(),
            cost_usd,
        });
        for id in dropped {
            host::delete_file(&session_file(id));
        }
        self.save_index();
    }

    /// The transcript of session `id`. If its file is gone or unreadable,
    /// the dangling history entry is dropped and `None` returned.
    pub(crate) fn open(&mut self, id: u64) -> Option<Transcript> {
        match host::read_json::<SavedSession>(&session_file(id)) {
            Some(session) => Some(session.messages),
            None => {
                self.forget(id);
                None
            }
        }
    }

    pub(crate) fn delete(&mut self, id: u64) {
        host::delete_file(&session_file(id));
        self.forget(id);
    }

    fn forget(&mut self, id: u64) {
        self.sessions.retain(|s| s.id != id);
        self.save_index();
    }

    fn save_index(&self) {
        let _ = host::write_json(INDEX_FILE, self);
    }

    /// Puts `meta` at the top of the list (replacing any older entry for the
    /// same session) and trims the list, returning the ids that fell off.
    fn record(&mut self, meta: SessionMeta) -> Vec<u64> {
        self.sessions.retain(|s| s.id != meta.id);
        self.sessions.insert(0, meta);
        let keep = self.sessions.len().min(MAX_SAVED_SESSIONS);
        self.sessions
            .split_off(keep)
            .into_iter()
            .map(|s| s.id)
            .collect()
    }
}
