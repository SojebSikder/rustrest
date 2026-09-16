//! End-to-end check of the Insomnia import/export plugin against the real
//! wasm build. Requires `rustrest-plugin-insomnia` to have been built first:
//!
//! ```text
//! cd crates/rustrest-plugin-insomnia
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! Skips itself (rather than failing) if that build output isn't present,
//! so a plain `cargo test --workspace` doesn't require the wasm32 target.

use rustrest_plugin_host::PluginManager;
use std::path::PathBuf;

fn wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-plugin-insomnia")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("rustrest_plugin_insomnia.wasm")
}

fn with_manager(f: impl FnOnce(&mut PluginManager)) {
    let wasm_path = wasm_path();
    if !wasm_path.is_file() {
        eprintln!(
            "skipping: insomnia plugin not built at {}; see file header for build instructions",
            wasm_path.display()
        );
        return;
    }

    let tmp = std::env::temp_dir().join(format!(
        "rustrest-plugin-host-insomnia-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let plugins_dir = tmp.join("plugins");
    let plugin_dir = plugins_dir.join("insomnia");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::copy(&wasm_path, plugin_dir.join("plugin.wasm")).unwrap();

    let mut manager = PluginManager::with_dirs(plugins_dir, tmp.join("plugins.json")).unwrap();
    manager.load_all();

    let installed = manager.installed();
    assert_eq!(installed.len(), 1, "expected exactly one discovered plugin");
    assert!(
        installed[0].load_error.is_none(),
        "plugin failed to load: {:?}",
        installed[0].load_error
    );

    f(&mut manager);

    std::fs::remove_dir_all(&tmp).ok();
}

const V4_SAMPLE: &str = r#"{
  "_type": "export",
  "__export_format": 4,
  "__export_date": "2024-01-01T00:00:00.000Z",
  "__export_source": "insomnia.desktop.app:v8.0.0",
  "resources": [
    {
      "_id": "wrk_1",
      "parentId": null,
      "name": "Demo Workspace",
      "scope": "collection",
      "_type": "workspace"
    },
    {
      "_id": "fld_1",
      "parentId": "wrk_1",
      "name": "Users",
      "metaSortKey": 1,
      "_type": "request_group"
    },
    {
      "_id": "req_1",
      "parentId": "fld_1",
      "name": "Get User",
      "method": "GET",
      "url": "{{ _.base_url }}/users/1",
      "headers": [{ "name": "Accept", "value": "application/json", "disabled": false }],
      "body": {},
      "metaSortKey": 1,
      "_type": "request"
    },
    {
      "_id": "req_2",
      "parentId": "wrk_1",
      "name": "Create User",
      "method": "POST",
      "url": "{{ _.base_url }}/users",
      "headers": [{ "name": "Content-Type", "value": "application/json", "disabled": false }],
      "body": { "mimeType": "application/json", "text": "{\"name\":\"a\"}" },
      "metaSortKey": 2,
      "_type": "request"
    },
    {
      "_id": "env_1",
      "parentId": "wrk_1",
      "name": "Base Environment",
      "data": { "base_url": "https://api.example.com" },
      "_type": "environment"
    }
  ]
}"#;

const V5_SAMPLE: &str = r#"
type: collection.insomnia.rest/5.0
name: Demo Workspace
collection:
  - name: Users
    children:
      - name: Get User
        url: "{{ _.base_url }}/users/1"
        method: GET
        headers:
          - name: Accept
            value: application/json
            disabled: false
  - name: Create User
    url: "{{ _.base_url }}/users"
    method: POST
    headers:
      - name: Content-Type
        value: application/json
    body:
      mimeType: application/json
      text: '{"name":"a"}'
environments:
  name: Base Environment
  data:
    base_url: https://api.example.com
"#;

#[test]
fn imports_v4_json_export() {
    with_manager(|manager| {
        let value = manager
            .import("insomnia", "insomnia", V4_SAMPLE.as_bytes().to_vec())
            .expect("v4 import should succeed");

        assert_eq!(value["info"]["name"], "Demo Workspace");
        let items = value["item"].as_array().expect("item array");
        assert_eq!(
            items.len(),
            2,
            "expected the Users folder + top-level request"
        );

        let folder = &items[0];
        assert_eq!(folder["name"], "Users");
        let nested = folder["item"].as_array().expect("nested items");
        assert_eq!(nested[0]["name"], "Get User");
        assert_eq!(nested[0]["request"]["method"], "GET");
        assert_eq!(nested[0]["request"]["url"], "{{base_url}}/users/1");

        let top_request = &items[1];
        assert_eq!(top_request["name"], "Create User");
        assert_eq!(top_request["request"]["body"]["mode"], "raw");
        assert_eq!(top_request["request"]["body"]["raw"], "{\"name\":\"a\"}");

        let vars = value["variable"].as_array().expect("variables");
        assert!(
            vars.iter()
                .any(|v| v["key"] == "base_url" && v["value"] == "https://api.example.com")
        );
    });
}

#[test]
fn imports_v5_yaml_export() {
    with_manager(|manager| {
        let value = manager
            .import("insomnia", "insomnia", V5_SAMPLE.as_bytes().to_vec())
            .expect("v5 import should succeed");

        assert_eq!(value["info"]["name"], "Demo Workspace");
        let items = value["item"].as_array().expect("item array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Users");
        assert_eq!(
            items[0]["item"][0]["request"]["url"],
            "{{base_url}}/users/1"
        );
        assert_eq!(items[1]["request"]["method"], "POST");

        let vars = value["variable"].as_array().expect("variables");
        assert!(
            vars.iter()
                .any(|v| v["key"] == "base_url" && v["value"] == "https://api.example.com")
        );
    });
}

#[test]
fn round_trips_through_v4_export_and_reimport() {
    with_manager(|manager| {
        let imported = manager
            .import("insomnia", "insomnia", V4_SAMPLE.as_bytes().to_vec())
            .unwrap();

        // give it the two required `#[serde(skip)]` fields the real
        // `PostmanCollection` struct would already have populated.
        let exported = manager
            .export("insomnia", "insomnia-v4", imported.clone())
            .expect("export should succeed");
        let reimported = manager
            .import("insomnia", "insomnia", exported)
            .expect("re-import of our own export should succeed");

        assert_eq!(reimported["info"]["name"], imported["info"]["name"]);
        assert_eq!(
            reimported["item"].as_array().unwrap().len(),
            imported["item"].as_array().unwrap().len()
        );
    });
}

#[test]
fn round_trips_through_v5_export_and_reimport() {
    with_manager(|manager| {
        let imported = manager
            .import("insomnia", "insomnia", V4_SAMPLE.as_bytes().to_vec())
            .unwrap();

        let exported = manager
            .export("insomnia", "insomnia-v5", imported.clone())
            .expect("export should succeed");
        let reimported = manager
            .import("insomnia", "insomnia", exported)
            .expect("re-import of our own export should succeed");

        assert_eq!(reimported["info"]["name"], imported["info"]["name"]);
        assert_eq!(
            reimported["item"].as_array().unwrap().len(),
            imported["item"].as_array().unwrap().len()
        );
    });
}
