//! Embedded terminal sessions backing `WorkspaceContent::Terminal` tabs.
//! Pure state container - opening/closing a terminal tab is tab lifecycle
//! (see `workbench`), and reconnecting one for a remote SSH session lives in
//! `remote`; both reach into `terminal_manager` here.

use rustrest_terminal::TerminalManager;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct TerminalState {
    pub terminal_manager: TerminalManager,
    /// clone of this and hand to every spawned session so its background I/O
    /// thread can report activity; the app-wide subscription drains the
    /// matching receiver and turns each notice into a `Message`.
    pub terminal_event_tx: UnboundedSender<(u64, rustrest_terminal::TerminalNotice)>,
    pub terminal_event_rx: Arc<Mutex<UnboundedReceiver<(u64, rustrest_terminal::TerminalNotice)>>>,
}
