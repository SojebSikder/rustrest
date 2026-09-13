use rustrest_terminal::{RemoteCommand, TerminalManager};

/// The remote backend should update the shared grid from fed bytes exactly
/// like a local PTY session would, without any real transport involved -
/// `feed()` stands in for "bytes just arrived from an SSH channel".
#[test]
fn feeds_remote_bytes_into_snapshot() {
    let mut manager = TerminalManager::new();
    let (id, mut feed, _rx) = manager.spawn_remote(80, 24, |_id, _notice| {});

    feed.feed(b"hello remote\r\n");

    let session = manager.get(id).expect("session should exist");
    let grid = session.snapshot();
    let text: String = grid.cells.iter().map(|c| c.c).collect();
    assert!(
        text.contains("hello remote"),
        "expected fed bytes to show up in the grid snapshot"
    );
}

/// writes and resizes on a remote session must be handed back as
/// `RemoteCommand`s instead of being applied to a local PTY, so the caller
/// (an SSH shell channel, in practice) can forward them to the real
/// transport.
#[test]
fn forwards_writes_and_resizes_as_remote_commands() {
    let mut manager = TerminalManager::new();
    let (id, _feed, mut rx) = manager.spawn_remote(80, 24, |_id, _notice| {});

    {
        let session = manager.get(id).expect("session should exist");
        session.write(b"ls\n".to_vec());
    }
    manager.resize(id, 100, 40, 8, 16);

    match rx.try_recv() {
        Ok(RemoteCommand::Write(bytes)) => assert_eq!(bytes, b"ls\n"),
        other => panic!("expected a Write command, got {other:?}"),
    }
    match rx.try_recv() {
        Ok(RemoteCommand::Resize(columns, rows)) => assert_eq!((columns, rows), (100, 40)),
        other => panic!("expected a Resize command, got {other:?}"),
    }
}
