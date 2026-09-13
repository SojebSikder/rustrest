use std::path::PathBuf;
use std::process::Stdio;

use rustrest_remote::{Request, Response, RpcClient};
use tokio::process::Command;

fn agent_binary() -> PathBuf {
    let mut target_dir = std::env::current_exe().expect("current_exe");
    target_dir.pop(); // .../target/<profile>/deps
    target_dir.pop(); // .../target/<profile>
    let exe_name = if cfg!(windows) {
        "rustrest-remote-agent.exe"
    } else {
        "rustrest-remote-agent"
    };
    let binary_path = target_dir.join(exe_name);

    if !binary_path.exists() {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "rustrest-remote-agent"])
            .status()
            .expect("failed to invoke cargo to build rustrest-remote-agent");
        assert!(status.success(), "building rustrest-remote-agent failed");
    }
    assert!(
        binary_path.exists(),
        "expected {binary_path:?} to exist after building rustrest-remote-agent"
    );
    binary_path
}

fn temp_workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rustrest_remote_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn spawn_agent() -> RpcClient {
    let mut child = Command::new(agent_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn rustrest-remote-agent");

    let stdout = child.stdout.take().unwrap();
    let stdin = child.stdin.take().unwrap();
    // the child is intentionally leaked here (not stored/awaited) - it exits
    // on its own once stdin closes, which happens when the RpcClient (and
    // its owned duplex stream) is dropped at the end of the test.
    std::mem::forget(child);

    RpcClient::spawn(tokio::io::join(stdout, stdin))
}

#[tokio::test]
async fn round_trips_file_operations_against_a_real_agent_process() {
    let workdir = temp_workdir();
    let rpc = spawn_agent().await;

    let file_path = workdir.join("hello.txt").to_string_lossy().into_owned();
    let subdir_path = workdir.join("subdir").to_string_lossy().into_owned();

    // write, then read back
    let response = rpc
        .call(Request::WriteFile(
            file_path.clone(),
            b"hello agent".to_vec(),
        ))
        .await
        .unwrap();
    assert!(matches!(response, Response::Ok), "unexpected: {response:?}");

    let response = rpc
        .call(Request::ReadFile(file_path.clone()))
        .await
        .unwrap();
    match response {
        Response::FileContent(bytes) => assert_eq!(bytes, b"hello agent"),
        other => panic!("unexpected: {other:?}"),
    }

    // create a directory, then list the workdir and find both entries
    let response = rpc
        .call(Request::CreateDir(subdir_path.clone()))
        .await
        .unwrap();
    assert!(matches!(response, Response::Ok), "unexpected: {response:?}");

    let response = rpc
        .call(Request::ListDir(workdir.to_string_lossy().into_owned()))
        .await
        .unwrap();
    let names: Vec<String> = match response {
        Response::DirListing(entries) => entries.into_iter().map(|e| e.name).collect(),
        other => panic!("unexpected: {other:?}"),
    };
    assert!(names.contains(&"hello.txt".to_string()));
    assert!(names.contains(&"subdir".to_string()));

    // rename, then delete
    let renamed_path = workdir.join("renamed.txt").to_string_lossy().into_owned();
    let response = rpc
        .call(Request::Rename(file_path.clone(), renamed_path.clone()))
        .await
        .unwrap();
    assert!(matches!(response, Response::Ok), "unexpected: {response:?}");

    let response = rpc.call(Request::Delete(renamed_path)).await.unwrap();
    assert!(matches!(response, Response::Ok), "unexpected: {response:?}");

    let response = rpc.call(Request::ReadFile(file_path)).await.unwrap();
    assert!(
        matches!(response, Response::Error(_)),
        "reading a deleted (renamed-away) file should error, got {response:?}"
    );

    std::fs::remove_dir_all(&workdir).ok();
}
