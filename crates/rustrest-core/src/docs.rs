//! markdown documentation for collections, folders and requests.
//!
//! every target (collection, folder or single request) is rendered by the
//! same `DocWriter`, so the section for a request looks identical whether it
//! is generated on its own or as part of its folder/collection. The GUI's
//! docs tab, "Export Docs" and any plugin/CLI caller all go through
//! [`generate`].

use crate::collection::model::{
    CollectionItem, PostmanBody, PostmanBodyRow, PostmanCollection, PostmanEvent, PostmanFolder,
    PostmanHeader, PostmanRequestNode, PostmanResponseExample, ProtocolRequestDetails,
};
use crate::collection::tree_ops::{find_folder, find_folder_mut, find_request, find_request_mut};
use std::collections::HashMap;
use std::fmt::Write as _;

/// which node of a collection a piece of documentation belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsTarget {
    Collection,
    /// folder names from the collection root down to the folder itself.
    Folder(Vec<String>),
    /// a request's runtime id (`PostmanRequestNode::id`).
    Request(usize),
}

/// toggles for what goes into generated documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocsOptions {
    /// a linked table of contents for collection/folder docs.
    pub include_toc: bool,
    /// saved response examples under each request.
    pub include_examples: bool,
    /// pre-request and test scripts under each request.
    pub include_scripts: bool,
}

impl Default for DocsOptions {
    fn default() -> Self {
        Self {
            include_toc: true,
            include_examples: true,
            include_scripts: false,
        }
    }
}

/// the markdown description stored on `target`, or `None` if the target no
/// longer exists in `collection`. An existing target without a description
/// yields `Some("")`.
pub fn description<'a>(collection: &'a PostmanCollection, target: &DocsTarget) -> Option<&'a str> {
    let desc = match target {
        DocsTarget::Collection => &collection.info.description,
        DocsTarget::Folder(path) => &find_folder(&collection.item, path)?.description,
        DocsTarget::Request(id) => &find_request(&collection.item, *id)?.request.description,
    };
    Some(desc.as_deref().unwrap_or(""))
}

/// stores `text` as `target`'s description (blank clears it) and flags the
/// edited node as unsaved. Returns `false` if the target no longer exists.
pub fn set_description(
    collection: &mut PostmanCollection,
    target: &DocsTarget,
    text: &str,
) -> bool {
    let value = (!text.trim().is_empty()).then(|| text.to_string());
    match target {
        DocsTarget::Collection => collection.info.description = value,
        DocsTarget::Folder(path) => {
            let Some(folder) = find_folder_mut(&mut collection.item, path) else {
                return false;
            };
            folder.description = value;
            folder.unsaved = true;
        }
        DocsTarget::Request(id) => {
            let Some(node) = find_request_mut(&mut collection.item, *id) else {
                return false;
            };
            node.request.description = value;
            node.unsaved = true;
        }
    }
    collection.unsaved = true;
    true
}

/// display name of `target`, or `None` if it no longer exists.
pub fn target_name(collection: &PostmanCollection, target: &DocsTarget) -> Option<String> {
    Some(match target {
        DocsTarget::Collection => collection.info.name.clone(),
        DocsTarget::Folder(path) => find_folder(&collection.item, path)?.name.clone(),
        DocsTarget::Request(id) => find_request(&collection.item, *id)?.name.clone(),
    })
}

/// generates markdown docs for `target`, or `None` if it no longer exists.
pub fn generate(
    collection: &PostmanCollection,
    target: &DocsTarget,
    options: &DocsOptions,
) -> Option<String> {
    Some(match target {
        DocsTarget::Collection => collection_markdown(collection, options),
        DocsTarget::Folder(path) => folder_markdown(find_folder(&collection.item, path)?, options),
        DocsTarget::Request(id) => request_markdown(find_request(&collection.item, *id)?, options),
    })
}

/// full docs for a collection: description, variables, contents, then every
/// folder/request in tree order.
pub fn collection_markdown(collection: &PostmanCollection, options: &DocsOptions) -> String {
    let mut w = DocWriter::new(*options);
    w.title(&collection.info.name);
    w.description(collection.info.description.as_deref());
    w.variables(collection);
    w.items(&collection.item, 2);
    w.finish()
}

/// docs for one folder and everything below it.
pub fn folder_markdown(folder: &PostmanFolder, options: &DocsOptions) -> String {
    let mut w = DocWriter::new(*options);
    w.title(&folder.name);
    w.description(folder.description.as_deref());
    w.items(&folder.item, 2);
    w.finish()
}

/// docs for a single request.
pub fn request_markdown(node: &PostmanRequestNode, options: &DocsOptions) -> String {
    let mut w = DocWriter::new(DocsOptions {
        include_toc: false,
        ..*options
    });
    w.title(&node.name);
    w.request_details(node);
    w.finish()
}

struct TocEntry {
    level: usize,
    title: String,
    anchor: String,
}

/// accumulates a document as `head` (title/description/variables) + a table
/// of contents + `body`, keeping heading anchors unique the way GitHub does.
struct DocWriter {
    options: DocsOptions,
    head: String,
    body: String,
    toc: Vec<TocEntry>,
    anchors: HashMap<String, usize>,
}

impl DocWriter {
    fn new(options: DocsOptions) -> Self {
        Self {
            options,
            head: String::new(),
            body: String::new(),
            toc: Vec::new(),
            anchors: HashMap::new(),
        }
    }

    fn title(&mut self, name: &str) {
        self.anchor(name);
        let _ = writeln!(self.head, "# {}\n", heading_text(name));
    }

    fn description(&mut self, description: Option<&str>) {
        write_description(&mut self.head, description);
    }

    fn variables(&mut self, collection: &PostmanCollection) {
        let rows: Vec<[String; 2]> = collection
            .variable
            .iter()
            .flatten()
            .filter(|v| v.r#type.as_deref() != Some("disabled"))
            .map(|v| {
                let value = match &v.value {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                [v.key.clone(), value]
            })
            .collect();
        if !rows.is_empty() {
            self.head.push_str("**Variables**\n\n");
            table(&mut self.head, &["Key", "Value"], &rows);
        }
    }

    fn items(&mut self, items: &[CollectionItem], level: usize) {
        for item in items {
            match item {
                CollectionItem::Folder(folder) => {
                    self.heading(level, &folder.name);
                    write_description(&mut self.body, folder.description.as_deref());
                    self.items(&folder.item, level + 1);
                }
                CollectionItem::Request(node) => {
                    self.heading(level, &node.name);
                    self.request_details(node);
                }
            }
        }
    }

    fn heading(&mut self, level: usize, name: &str) {
        let anchor = self.anchor(name);
        let _ = writeln!(
            self.body,
            "{} {}\n",
            "#".repeat(level.min(6)),
            heading_text(name)
        );
        self.toc.push(TocEntry {
            level,
            title: heading_text(name),
            anchor,
        });
    }

    /// writes everything below a request's heading into `body`.
    fn request_details(&mut self, node: &PostmanRequestNode) {
        let out = &mut self.body;
        let details = &node.request;

        match &node.protocol_request {
            Some(ProtocolRequestDetails::WebSocket(ws)) => {
                code_block(out, &format!("WEBSOCKET {}", ws.url), "http");
            }
            Some(ProtocolRequestDetails::GraphQl(gql)) => {
                code_block(out, &format!("GRAPHQL {}", gql.url), "http");
            }
            Some(ProtocolRequestDetails::Grpc(grpc)) => {
                let target = match (grpc.service.is_empty(), grpc.method.is_empty()) {
                    (false, false) => format!("{} {}/{}", grpc.endpoint, grpc.service, grpc.method),
                    _ => grpc.endpoint.clone(),
                };
                code_block(out, &format!("GRPC {target}"), "http");
            }
            None => {
                let url = details
                    .url
                    .as_ref()
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                code_block(out, format!("{} {url}", details.method).trim_end(), "http");
            }
        }

        write_description(out, details.description.as_deref());

        if let Some(auth) = details
            .auth
            .as_ref()
            .filter(|a| a.auth_type != crate::AuthType::NoAuth)
        {
            let _ = writeln!(out, "**Authorization:** {}\n", auth.auth_type.label());
        }

        let headers = match &node.protocol_request {
            Some(ProtocolRequestDetails::WebSocket(ws)) => Some(ws.headers.as_slice()),
            Some(ProtocolRequestDetails::GraphQl(gql)) => Some(gql.headers.as_slice()),
            Some(ProtocolRequestDetails::Grpc(grpc)) => Some(grpc.metadata.as_slice()),
            None => details.header.as_deref(),
        };
        let header_rows = header_rows(headers.unwrap_or_default());
        if !header_rows.is_empty() {
            let label = if matches!(node.protocol_request, Some(ProtocolRequestDetails::Grpc(_))) {
                "Metadata"
            } else {
                "Headers"
            };
            let _ = writeln!(out, "**{label}**\n");
            table(out, &["Key", "Value"], &header_rows);
        }

        if node.protocol_request.is_none() {
            let url = details
                .url
                .as_ref()
                .map(|u| u.to_string())
                .unwrap_or_default();
            let params = query_params(&url);
            if !params.is_empty() {
                out.push_str("**Query Parameters**\n\n");
                table(out, &["Key", "Value"], &params);
            }
        }

        match &node.protocol_request {
            Some(ProtocolRequestDetails::GraphQl(gql)) => {
                graphql_body(out, &gql.query, Some(gql.variables.as_str()));
            }
            Some(ProtocolRequestDetails::Grpc(grpc)) if !grpc.request_json.trim().is_empty() => {
                out.push_str("**Request Message**\n\n");
                code_block(out, &grpc.request_json, "json");
            }
            Some(_) => {}
            None => {
                if let Some(body) = &details.body {
                    request_body(out, body);
                }
            }
        }

        if self.options.include_scripts {
            scripts(out, node.event.as_deref().unwrap_or_default());
        }

        if self.options.include_examples {
            examples(out, node.response.as_deref().unwrap_or_default());
        }
    }

    /// registers `name` as a heading and returns its unique anchor slug.
    fn anchor(&mut self, name: &str) -> String {
        let base = slug(name);
        let count = self.anchors.entry(base.clone()).or_insert(0);
        let anchor = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        anchor
    }

    fn finish(mut self) -> String {
        let mut out = std::mem::take(&mut self.head);
        if self.options.include_toc && !self.toc.is_empty() {
            let min_level = self.toc.iter().map(|e| e.level).min().unwrap_or(2);
            out.push_str("**Contents**\n\n");
            for entry in &self.toc {
                let indent = "  ".repeat(entry.level - min_level);
                let _ = writeln!(out, "{indent}- [{}](#{})", entry.title, entry.anchor);
            }
            out.push('\n');
        }
        out.push_str(&self.body);
        let trimmed = out.trim_end().len();
        out.truncate(trimmed);
        out.push('\n');
        out
    }
}

fn write_description(out: &mut String, description: Option<&str>) {
    if let Some(desc) = description.map(str::trim).filter(|d| !d.is_empty()) {
        let _ = writeln!(out, "{desc}\n");
    }
}

fn header_rows(headers: &[PostmanHeader]) -> Vec<[String; 2]> {
    headers
        .iter()
        .filter(|h| h.disabled != Some(true) && !h.key.is_empty())
        .map(|h| [h.key.clone(), h.value.clone()])
        .collect()
}

fn request_body(out: &mut String, body: &PostmanBody) {
    match body.mode.as_deref() {
        Some("raw") => {
            if let Some(raw) = body.raw.as_deref().filter(|r| !r.trim().is_empty()) {
                out.push_str("**Body**\n\n");
                code_block(out, raw, guess_language(raw));
            }
        }
        Some("urlencoded") => {
            let rows = body_rows(body.urlencoded.as_deref().unwrap_or_default(), false);
            if !rows.is_empty() {
                out.push_str("**Body** (`x-www-form-urlencoded`)\n\n");
                table(out, &["Key", "Value"], &rows);
            }
        }
        Some("formdata") => {
            let rows = body_rows(body.formdata.as_deref().unwrap_or_default(), true);
            if !rows.is_empty() {
                out.push_str("**Body** (`form-data`)\n\n");
                table(out, &["Key", "Value", "Type"], &rows);
            }
        }
        Some("file") => {
            let file_name = body
                .file
                .as_ref()
                .and_then(|f| f.src.as_deref())
                .filter(|src| !src.is_empty())
                .map(|src| {
                    std::path::Path::new(src)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(src)
                });
            if let Some(file_name) = file_name {
                let _ = writeln!(
                    out,
                    "**Body** (`binary`): `{file_name}`
"
                );
            }
        }
        Some("graphql") => {
            if let Some(gql) = &body.graphql {
                graphql_body(out, &gql.query, gql.variables.as_deref());
            }
        }
        _ => {}
    }
}

fn body_rows(rows: &[PostmanBodyRow], with_type: bool) -> Vec<Vec<String>> {
    rows.iter()
        .filter(|r| r.disabled != Some(true) && !r.key.is_empty())
        .map(|r| {
            // file rows show their file names rather than full local paths
            let value = match &r.src {
                Some(src) if r.r#type.as_deref() == Some("file") => src
                    .paths()
                    .iter()
                    .map(|p| {
                        std::path::Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => r.value.clone().unwrap_or_default(),
            };
            let mut cells = vec![r.key.clone(), value];
            if with_type {
                let kind = r.r#type.clone().unwrap_or_else(|| "text".to_string());
                cells.push(match r.content_type.as_deref().filter(|c| !c.is_empty()) {
                    Some(content_type) => format!("{kind} ({content_type})"),
                    None => kind,
                });
            }
            cells
        })
        .collect()
}

fn graphql_body(out: &mut String, query: &str, variables: Option<&str>) {
    if !query.trim().is_empty() {
        out.push_str("**Query**\n\n");
        code_block(out, query, "graphql");
    }
    if let Some(vars) = variables.filter(|v| !v.trim().is_empty() && v.trim() != "{}") {
        out.push_str("**Variables**\n\n");
        code_block(out, vars, "json");
    }
}

fn scripts(out: &mut String, events: &[PostmanEvent]) {
    for event in events {
        let Some(exec) = event.script.as_ref().and_then(|s| s.exec.as_ref()) else {
            continue;
        };
        let code = exec.to_string_contents();
        if code.trim().is_empty() {
            continue;
        }
        let label = match event.listen.as_str() {
            "prerequest" => "Pre-request Script",
            "test" => "Post-response Script",
            other => other,
        };
        let _ = writeln!(out, "**{label}**\n");
        code_block(out, &code, "javascript");
    }
}

fn examples(out: &mut String, responses: &[PostmanResponseExample]) {
    if responses.is_empty() {
        return;
    }
    out.push_str("**Example Responses**\n\n");
    for example in responses {
        let _ = writeln!(
            out,
            "_{}_ — `{}`\n",
            heading_text(&example.name),
            example.code
        );
        if let Some(body) = example.body.as_deref().filter(|b| !b.trim().is_empty()) {
            let lang = guess_language(body);
            let pretty = if lang == "json" {
                serde_json::from_str::<serde_json::Value>(body)
                    .and_then(|v| serde_json::to_string_pretty(&v))
                    .unwrap_or_else(|_| body.to_string())
            } else {
                body.to_string()
            };
            code_block(out, &pretty, lang);
        }
    }
}

/// splits a URL's query string into key/value rows without requiring the URL
/// to parse (collection URLs are often `{{base_url}}/path?x={{var}}`).
fn query_params(url: &str) -> Vec<[String; 2]> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            [k.to_string(), v.to_string()]
        })
        .collect()
}

fn guess_language(content: &str) -> &'static str {
    let trimmed = content.trim_start();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(content).is_ok()
    {
        "json"
    } else if trimmed.starts_with('<') {
        "xml"
    } else {
        ""
    }
}

/// writes a fenced code block, lengthening the fence if `content` itself
/// contains backtick runs.
fn code_block(out: &mut String, content: &str, lang: &str) {
    let longest_run = content.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    let _ = writeln!(out, "{fence}{lang}\n{}\n{fence}\n", content.trim_end());
}

fn table<R: AsRef<[String]>>(out: &mut String, headers: &[&str], rows: &[R]) {
    let _ = writeln!(out, "| {} |", headers.join(" | "));
    let _ = writeln!(out, "|{}", " --- |".repeat(headers.len()));
    for row in rows {
        let cells: Vec<String> = row.as_ref().iter().map(|c| table_cell(c)).collect();
        let _ = writeln!(out, "| {} |", cells.join(" | "));
    }
    out.push('\n');
}

fn table_cell(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    format!(
        "`{}`",
        value
            .replace('|', "\\|")
            .replace("\r\n", " ")
            .replace(['\n', '`'], " ")
    )
}

/// item names are free text; keep them from turning into markdown syntax
/// when used as a heading or link label.
fn heading_text(name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "Untitled" } else { name };
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '[' | ']' | '*' | '_' | '`' | '#' | '<' | '>' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// GitHub-style heading anchor: lowercase, spaces to `-`, punctuation dropped.
fn slug(name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "Untitled" } else { name };
    name.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c.to_lowercase().collect::<String>())
            } else if c == ' ' {
                Some("-".to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::model::{
        CollectionInfo, PostmanRequestDetails, PostmanUrl, PostmanVariable,
    };

    fn request(id: usize, name: &str, method: &str, url: &str) -> CollectionItem {
        CollectionItem::Request(PostmanRequestNode {
            id,
            name: name.to_string(),
            event: None,
            request: PostmanRequestDetails {
                method: method.to_string(),
                url: Some(PostmanUrl::String(url.to_string())),
                header: Some(vec![
                    PostmanHeader {
                        key: "Accept".to_string(),
                        value: "application/json".to_string(),
                        disabled: None,
                    },
                    PostmanHeader {
                        key: "X-Off".to_string(),
                        value: "1".to_string(),
                        disabled: Some(true),
                    },
                ]),
                body: None,
                auth: None,
                description: Some(format!("Docs for **{name}**.")),
            },
            unsaved: false,
            response: None,
            protocol_request: None,
        })
    }

    fn folder(name: &str, item: Vec<CollectionItem>) -> CollectionItem {
        CollectionItem::Folder(PostmanFolder {
            name: name.to_string(),
            protocol_profile_behavior: None,
            item,
            event: None,
            description: Some(format!("About {name}")),
            unsaved: false,
        })
    }

    fn sample() -> PostmanCollection {
        PostmanCollection {
            id: 1,
            file_path: None,
            storage_dir: None,
            remote_dir: None,
            unsaved: false,
            info: CollectionInfo {
                name: "Pet Store".to_string(),
                postman_id: None,
                schema: String::new(),
                description: Some("The pet store API.".to_string()),
            },
            item: vec![
                folder(
                    "Users",
                    vec![
                        request(1, "Get User", "GET", "{{base}}/users/1?expand=pets&x"),
                        folder(
                            "Admin",
                            vec![request(2, "Get User", "DELETE", "{{base}}/u")],
                        ),
                    ],
                ),
                request(3, "Health", "GET", "{{base}}/health"),
            ],
            variable: Some(vec![PostmanVariable {
                key: "base".to_string(),
                value: Some(serde_json::Value::String("https://x.io".to_string())),
                r#type: None,
            }]),
        }
    }

    #[test]
    fn collection_docs_cover_whole_tree_with_unique_anchors() {
        let md = collection_markdown(&sample(), &DocsOptions::default());
        assert!(md.starts_with("# Pet Store\n\nThe pet store API.\n"));
        assert!(md.contains("| `base` | `https://x.io` |"));
        assert!(md.contains("- [Users](#users)\n  - [Get User](#get-user)\n  - [Admin](#admin)\n    - [Get User](#get-user-1)\n- [Health](#health)"));
        assert!(md.contains("## Users\n\nAbout Users\n\n### Get User\n"));
        assert!(md.contains("#### Get User\n\n```http\nDELETE {{base}}/u\n```"));
        assert!(md.contains("| `expand` | `pets` |\n| `x` |  |"));
        assert!(md.contains("| `Accept` | `application/json` |"));
        assert!(!md.contains("X-Off"));
    }

    #[test]
    fn folder_and_request_reuse_the_same_sections() {
        let col = sample();
        let folder_md = generate(
            &col,
            &DocsTarget::Folder(vec!["Users".into(), "Admin".into()]),
            &DocsOptions::default(),
        )
        .unwrap();
        assert!(folder_md.starts_with("# Admin\n\nAbout Admin\n"));
        assert!(folder_md.contains("## Get User\n\n```http\nDELETE {{base}}/u\n```"));

        let req_md = generate(&col, &DocsTarget::Request(3), &DocsOptions::default()).unwrap();
        assert!(req_md.starts_with(
            "# Health\n\n```http\nGET {{base}}/health\n```\n\nDocs for **Health**.\n"
        ));
        assert!(!req_md.contains("**Contents**"));

        assert!(generate(&col, &DocsTarget::Request(99), &DocsOptions::default()).is_none());
    }

    #[test]
    fn set_description_updates_target_and_flags_unsaved() {
        let mut col = sample();
        let target = DocsTarget::Folder(vec!["Users".into()]);
        assert!(set_description(&mut col, &target, "# New"));
        assert_eq!(description(&col, &target), Some("# New"));
        assert!(col.unsaved);

        assert!(set_description(&mut col, &DocsTarget::Request(2), "  "));
        assert_eq!(description(&col, &DocsTarget::Request(2)), Some(""));
        assert!(!set_description(
            &mut col,
            &DocsTarget::Folder(vec!["Nope".into()]),
            "x"
        ));
    }

    #[test]
    fn code_fence_grows_past_embedded_backticks() {
        let mut out = String::new();
        code_block(&mut out, "a ``` b", "");
        assert_eq!(out, "````\na ``` b\n````\n\n");
    }
}
