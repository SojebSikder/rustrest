//! drives the real `rustrest-js-lsp` server through the client. Requires it
//! to have been built first (it's a standalone crate, outside this workspace):
//!
//! ```text
//! cd crates/rustrest-js-lsp
//! cargo build --release
//! ```
//!
//! Skips itself (rather than failing) if the binary isn't there.

use rustrest_lsp::{Event, LanguageServer, Position, ServerCommand, Severity};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn server_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-js-lsp")
        .join("target")
        .join("release")
        .join(format!("rustrest-js-lsp{}", std::env::consts::EXE_SUFFIX))
}

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

#[test]
fn completes_hovers_and_diagnoses_pm_scripts() {
    let path = server_path();
    if !path.is_file() {
        eprintln!("skipping: {} not built; see file header", path.display());
        return;
    }
    let command = ServerCommand {
        command: path.to_string_lossy().to_string(),
        ..ServerCommand::default()
    };
    let mut server = LanguageServer::start("rustrest-js-lsp", &command, None).unwrap();
    let mut backlog = Vec::new();

    server.sync_document(
        URI,
        "javascript",
        1,
        "const héllo = 1;\npm.enviroment.get(\"x\");\n",
    );
    wait_for(&mut server, &mut backlog, |e| {
        matches!(e, Event::Ready).then_some(())
    });
    let diagnostics = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Diagnostics {
            version: Some(1),
            diagnostics,
            ..
        } => Some(diagnostics.clone()),
        _ => None,
    });
    let typo = diagnostics
        .iter()
        .find(|d| d.message.contains("enviroment"))
        .expect("unknown-member warning");
    assert_eq!(typo.severity, Severity::Warning);
    assert_eq!(typo.range.start, Position { line: 1, column: 3 });

    // mid-typing: a syntax error, but completions still work
    server.sync_document(
        URI,
        "javascript",
        2,
        "const héllo = 1;\npm.enviroment.get(\"x\");\npm.",
    );
    let request = server
        .completion(URI, Position { line: 2, column: 3 })
        .unwrap();
    let items = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Completions { request: r, items } if *r == request => Some(items.clone()),
        _ => None,
    });
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for expected in ["test", "expect", "environment", "response"] {
        assert!(
            labels.contains(&expected),
            "{expected} missing from {labels:?}"
        );
    }

    server.sync_document(URI, "javascript", 3, "pm.environment.get(\"x\");");
    let request = server
        .hover(
            URI,
            Position {
                line: 0,
                column: 16,
            },
        )
        .unwrap();
    let hover = wait_for(&mut server, &mut backlog, |e| match e {
        Event::Hover {
            request: r,
            contents,
        } if *r == request => Some(contents.clone()),
        _ => None,
    })
    .expect("hover contents");
    assert!(hover.contains("pm.environment.get"), "{hover}");
}
