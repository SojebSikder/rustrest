use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustrest_terminal::TerminalManager;

#[test]
fn spawns_shell_and_echoes_input() {
    let mut manager = TerminalManager::new();
    let dirty = Arc::new(Mutex::new(false));
    let dirty_writer = dirty.clone();

    let id = manager
        .spawn(80, 24, move |_id, _notice| {
            *dirty_writer.lock().unwrap() = true;
        })
        .expect("failed to spawn terminal session");

    let marker = "RUSTREST_SMOKE_TEST_MARKER";
    let session = manager
        .get(id)
        .expect("session should exist right after spawn");
    session.write(format!("echo {marker}\r\n").into_bytes());

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = false;
    while Instant::now() < deadline {
        let session = manager.get(id).expect("session should still exist");
        let grid = session.snapshot();
        let text: String = grid.cells.iter().map(|c| c.c).collect();
        if text.contains(marker) {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        found,
        "expected the shell's echoed output to contain the marker"
    );

    manager.close(id);
}
