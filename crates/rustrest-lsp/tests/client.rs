//! drives `examples/fake_server.rs` (built by cargo alongside the tests)
//! through the real process/thread/JSON-RPC path.

use rustrest_lsp::{CompletionKind, Event, LanguageServer, Position, ServerCommand, Severity};
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn fake_server(utf8: bool) -> ServerCommand {
    // target/<profile>/deps/client-<hash> -> target/<profile>/examples/fake_server
    let exe = std::env::current_exe().unwrap();
    let dir: PathBuf = exe.parent().unwrap().parent().unwrap().join("examples");
    let command = dir.join(format!("fake_server{}", std::env::consts::EXE_SUFFIX));
    let env = if utf8 {
        vec![("FAKE_LSP_UTF8".to_string(), "1".to_string())]
    } else {
        Vec::new()
    };
    ServerCommand {
        command: command.to_string_lossy().to_string(),
        args: Vec::new(),
        env,
    }
}

/// polls until `pick` matches an event; events it passes over stay queued
/// for later calls.
fn wait_for<T>(
    server: &mut LanguageServer,
    backlog: &mut Vec<Event>,
    mut pick: impl FnMut(&Event) -> Option<T>,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        backlog.extend(server.poll());
        if let Some(index) = backlog.iter().position(|e| pick(e).is_some()) {
            let event = backlog.remove(index);
            return pick(&event).unwrap();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for an event; saw {backlog:?}");
}

const URI: &str = "file:///rustrest/scripts/tab-1-post/post-response.js";

fn run(utf8: bool) {
    let mut server =
        LanguageServer::start("fake", &fake_server(utf8), Some(json!({ "answer": 42 }))).unwrap();
    let mut backlog = Vec::new();
    assert!(!server.is_ready());
    // opened before initialization finishes: queued, then flushed
    server.sync_document(URI, "javascript", 1, "const é = 1;\n😀 bad");
    assert_eq!(server.completion(URI, Position::default()), None);

    wait_for(&mut server, &mut backlog, |e| {
        matches!(e, Event::Ready).then_some(())
    });
    assert!(server.is_ready());

    let diagnostics = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Diagnostics {
            uri,
            version: Some(1),
            diagnostics,
        } if uri == URI => Some(diagnostics.clone()),
        _ => None,
    });
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(diagnostics[0].message, "bad word (fake)");
    // `😀 ` is 5 UTF-8 bytes (3 UTF-16 units) before `bad`
    assert_eq!(diagnostics[0].range.start, Position { line: 1, column: 5 });
    assert_eq!(diagnostics[0].range.end, Position { line: 1, column: 8 });

    // cursor after `😀 b`: byte 6 -> utf-16 unit 4
    let request = server
        .completion(URI, Position { line: 1, column: 6 })
        .unwrap();
    let items = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Completions { request: r, items } if *r == request => Some(items.clone()),
        _ => None,
    });
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    let expected_column = if utf8 { 6 } else { 4 };
    assert_eq!(
        labels,
        vec!["alpha", "beta", &format!("at:1:{expected_column}")]
    );
    assert_eq!(items[0].kind, CompletionKind::Function);
    assert_eq!(items[1].documentation.as_deref(), Some("B"));

    let request = server.hover(URI, Position { line: 0, column: 8 }).unwrap();
    let hover = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Hover {
            request: r,
            contents,
        } if *r == request => Some(contents.clone()),
        _ => None,
    });
    let expected_column = if utf8 { 8 } else { 7 };
    assert_eq!(
        hover.as_deref(),
        Some(format!("hover 0:{expected_column} cfg=[{{\"answer\":42}}]").as_str())
    );

    // same version: no resend; a new version re-publishes
    server.sync_document(URI, "javascript", 1, "ignored");
    server.sync_document(URI, "javascript", 2, "fine");
    let diagnostics = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Diagnostics {
            version: Some(2),
            diagnostics,
            ..
        } => Some(diagnostics.clone()),
        _ => None,
    });
    assert!(diagnostics.is_empty());
}

#[test]
fn round_trip_with_utf16_server() {
    run(false);
}

#[test]
fn round_trip_with_utf8_server() {
    run(true);
}

#[test]
fn reports_stderr_and_exit() {
    let mut server = LanguageServer::start("fake", &fake_server(false), None).unwrap();
    let mut backlog = Vec::new();
    let log = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Log(line) => Some(line.clone()),
        _ => None,
    });
    assert_eq!(log, "[fake] fake server starting");

    server.sync_document(URI, "javascript", 1, "ok");
    wait_for(&mut server, &mut backlog, |e| {
        matches!(e, Event::Ready).then_some(())
    });
    server.sync_document(URI, "javascript", 2, "crash");
    let code = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Exited(code) => Some(*code),
        _ => None,
    });
    assert_eq!(code, Some(3));
    assert!(!server.is_running());
    assert_eq!(server.completion(URI, Position::default()), None);
}

#[test]
fn missing_binary_is_an_error() {
    let command = ServerCommand {
        command: "definitely-not-a-real-language-server".to_string(),
        ..ServerCommand::default()
    };
    assert!(LanguageServer::start("missing", &command, None).is_err());
}
