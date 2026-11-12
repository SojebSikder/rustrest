//! The system prompt: what the agent can do, what the user has open, the
//! context they've opted into, and the reply formats the app knows how to act on.

use rustrest_plugin_api::{
    CollectionSummary, PickedFile, ProtocolLogEntry, ProtocolRequestContext, RightPanelContext,
};
use std::collections::HashSet;

/// Most request lines listed in the collection overview - focused
/// collections are listed first, so they're the last to be cut.
const MAX_PROMPT_REQUESTS: usize = 400;

/// Who the agent is and what it can do.
const INTRO: &str =
    "You are an AI assistant embedded in Rustrest, an API client for HTTP/REST, WebSocket, \
     GraphQL and gRPC, docked next to whatever request the user has open. You can: answer \
     questions, explain responses and errors in plain language, edit the active request \
     (of any protocol), write post-response test scripts for HTTP requests, and create or \
     change collections, folders and requests. Decide what's needed from the user's message \
     - don't ask them to pick a mode.";

/// How to answer, and the fenced-block formats [`crate::actions`] parses
/// request edits and collection changes from.
const REPLY_FORMAT: &str = "\n\nAnswer concisely; markdown formatting is fine.\n\n\
     EDITING THE ACTIVE REQUEST: if, and only if, the user's message asks you to change the \
     active request or add/replace its pre-request or post-response script, follow your \
     explanation with exactly one fenced ```request-patch code block containing ONLY the \
     fields that should change, matching this shape: {\"method\":string|null,\
     \"url\":string|null,\"headers\":[[string,string]]|null,\"body\":string|null,\
     \"pre_request_script\":string|null,\"post_response_script\":string|null,\
     \"query\":string|null,\"variables\":string|null,\"operation_name\":string|null,\
     \"grpc_service\":string|null,\"grpc_method\":string|null,\"use_tls\":bool|null}. \
     Omit fields you don't want to change (or set them to null); `headers` replaces the \
     whole header list. Which fields apply depends on the active request's protocol:\n\
     - HTTP: method, url, headers, body, pre_request_script, post_response_script. Scripts \
     are JavaScript using the pm.* scripting API this app exposes (pm.test(name, fn), \
     pm.expect(...), pm.response.status/json(), pm.variables.get/set(...)).\n\
     - WebSocket: url, headers, and body = the message to send (it's put in the compose \
     box; the user sends it).\n\
     - GraphQL: url, headers, query (the GraphQL document), variables (a JSON object as a \
     string), operation_name.\n\
     - gRPC: url = the endpoint (host:port), use_tls, headers = metadata, grpc_service \
     (fully-qualified, e.g. \"helloworld.Greeter\"), grpc_method, and body = the request \
     message as JSON. Prefer services/methods from the discovered list when there is one.\n\
     Use a ```request-patch fence only for this - format any other example JSON with a \
     plain ```json fence.\n\n\
     CHANGING COLLECTIONS: if, and only if, the user's message asks you to create, rename, \
     update, delete, duplicate, or move a collection, folder, or request, follow your \
     explanation with exactly one fenced ```collection-action code block (instead of the \
     ```request-patch one - never both) containing either one JSON object or a JSON array of \
     them, applied in order. Each object is shaped as one of:\n\
     {\"op\":\"create_collection\",\"name\":string}\n\
     {\"op\":\"rename_collection\",\"collection_id\":number,\"new_name\":string}\n\
     {\"op\":\"delete_collection\",\"collection_id\":number}\n\
     {\"op\":\"create_folder\",\"collection_id\":number,\"parent_path\":[string],\"name\":string}\n\
     {\"op\":\"rename_folder\",\"collection_id\":number,\"path\":[string],\"new_name\":string}\n\
     {\"op\":\"delete_folder\",\"collection_id\":number,\"path\":[string]}\n\
     {\"op\":\"create_request\",\"collection_id\":number,\"parent_path\":[string],\"name\":string,\"protocol\":\"http\"|\"websocket\"|\"graphql\"|\"grpc\",\"method\":string,\"url\":string}\n\
     {\"op\":\"update_request\",\"collection_id\":number,\"request_id\":number, ...any request-patch fields...}\n\
     {\"op\":\"rename_request\",\"collection_id\":number,\"request_id\":number,\"new_name\":string}\n\
     {\"op\":\"delete_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
     {\"op\":\"duplicate_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
     {\"op\":\"move_request\",\"collection_id\":number,\"from_path\":[string],\"request_id\":number,\"to_path\":[string]}\n\
     `parent_path`/`path`/`from_path`/`to_path` are folder-name arrays (e.g. [] for the \
     collection root, [\"Auth\"] for a top-level folder named Auth). `protocol` defaults to \
     \"http\"; `method` only matters for HTTP, and for gRPC `url` is the endpoint. \
     update_request takes the same fields as a request-patch, mapped by the target \
     request's protocol as described above. Within one batch, a folder created earlier can \
     be used as a later parent_path, but a request's id is only known once it exists - so \
     update_request can only target requests listed above, not ones created in the same \
     batch.\n\
     RESOLVING IDS: you already know every collection - they're listed above under \
     \"Collections\". When the user names a collection, folder or request, match it to \
     that list yourself (case-insensitively, by the closest name) and use its id/path. \
     Never ask the user for an id - they don't see ids. If the user doesn't say which \
     collection and there is exactly one, use it; if it's genuinely ambiguous, ask by \
     name (\"which collection - Foo or Bar?\"). Never invent an id that isn't listed. \
     Deleting, moving, or updating something will ask the user to confirm before it's \
     applied; say so in your explanation. Otherwise, don't include this block at all.";

/// Extra context the user has opted into sending, beyond the active request
/// and the collection overview that are always included.
#[derive(Debug, Default)]
pub(crate) struct ContextSelection {
    /// ids of collections to list first and mark as the focus
    pub(crate) collections: HashSet<usize>,
    /// names of environments whose variables (values included) are sent -
    /// opt-in per name, since values may be secrets
    pub(crate) environments: HashSet<String>,
    /// files from "Attach Files", sent with every request until removed
    pub(crate) files: Vec<PickedFile>,
}

impl ContextSelection {
    pub(crate) fn set_collection(&mut self, id: usize, selected: bool) {
        if selected {
            self.collections.insert(id);
        } else {
            self.collections.remove(&id);
        }
    }

    pub(crate) fn set_environment(&mut self, name: &str, selected: bool) {
        if selected {
            self.environments.insert(name.to_string());
        } else {
            self.environments.remove(name);
        }
    }

    pub(crate) fn remove_file(&mut self, index: usize) {
        if index < self.files.len() {
            self.files.remove(index);
        }
    }
}

pub(crate) fn build_system_prompt(ctx: &RightPanelContext, selection: &ContextSelection) -> String {
    let mut prompt = String::from(INTRO);
    push_active_request(&mut prompt, ctx);
    push_collections_overview(&mut prompt, &ctx.collections, &selection.collections);
    push_selected_environments(&mut prompt, ctx, &selection.environments);
    push_attached_files(&mut prompt, &selection.files);
    prompt.push_str(REPLY_FORMAT);
    prompt
}

/// The request open in the active tab, of whichever protocol.
fn push_active_request(prompt: &mut String, ctx: &RightPanelContext) {
    if let Some(req) = &ctx.active_request {
        prompt.push_str(&format!(
            "\n\nActive request:\nmethod: {}\nurl: {}\nheaders: {:?}\nbody: {}",
            req.method, req.url, req.headers, req.body
        ));
    }
    if let Some(resp) = &ctx.active_response {
        prompt.push_str(&format!(
            "\n\nLast response:\nstatus: {}\nheaders: {:?}\nbody: {}",
            resp.status, resp.headers, resp.body
        ));
    }
    if let Some(protocol) = &ctx.active_protocol_request {
        push_protocol_context(prompt, protocol);
    }
}

fn push_selected_environments(
    prompt: &mut String,
    ctx: &RightPanelContext,
    selected: &HashSet<String>,
) {
    let mut envs = ctx
        .environments
        .iter()
        .filter(|e| selected.contains(&e.name))
        .peekable();
    if envs.peek().is_none() {
        return;
    }
    prompt.push_str("\n\nSelected environments (variable values included as-is):");
    for env in envs {
        prompt.push_str(&format!("\n- environment \"{}\"", env.name));
        for (key, value) in &env.variables {
            prompt.push_str(&format!("\n    {key} = {value}"));
        }
    }
}

fn push_attached_files(prompt: &mut String, files: &[PickedFile]) {
    if files.is_empty() {
        return;
    }
    prompt.push_str("\n\nAttached files:");
    for file in files {
        prompt.push_str(&format!("\n\n--- {} ---\n{}", file.name, file.text));
    }
}

fn push_protocol_log(prompt: &mut String, title: &str, log: &[ProtocolLogEntry]) {
    if log.is_empty() {
        return;
    }
    prompt.push_str(&format!("\n{title} (oldest first):"));
    for entry in log {
        prompt.push_str(&format!(
            "\n  [{} {}] {}",
            entry.direction, entry.label, entry.body
        ));
    }
}

/// active WebSocket/GraphQL/gRPC request, for the system prompt.
fn push_protocol_context(prompt: &mut String, protocol: &ProtocolRequestContext) {
    match protocol {
        ProtocolRequestContext::WebSocket {
            url,
            headers,
            message,
            connected,
            log,
        } => {
            prompt.push_str(&format!(
                "\n\nActive request (WebSocket):\nurl: {url}\nheaders: {headers:?}\n\
                 connected: {connected}\nmessage being composed: {message}"
            ));
            push_protocol_log(prompt, "Message log", log);
        }
        ProtocolRequestContext::GraphQl {
            url,
            headers,
            query,
            variables,
            operation_name,
            response,
            log,
        } => {
            prompt.push_str(&format!(
                "\n\nActive request (GraphQL):\nurl: {url}\nheaders: {headers:?}\n\
                 operation name: {operation_name}\nquery:\n{query}\nvariables: {variables}"
            ));
            if let Some(response) = response {
                prompt.push_str(&format!("\n\nLast response:\n{response}"));
            }
            push_protocol_log(prompt, "Subscription events", log);
        }
        ProtocolRequestContext::Grpc {
            endpoint,
            use_tls,
            metadata,
            service,
            method,
            request_json,
            methods,
            log,
        } => {
            prompt.push_str(&format!(
                "\n\nActive request (gRPC):\nendpoint: {endpoint}\ntls: {use_tls}\n\
                 metadata: {metadata:?}\nservice: {}\nmethod: {}\nrequest JSON: {request_json}",
                service.as_deref().unwrap_or("(none selected)"),
                method.as_deref().unwrap_or("(none selected)"),
            ));
            if methods.is_empty() {
                prompt.push_str("\nservices: not discovered yet");
            } else {
                prompt.push_str("\ndiscovered methods:");
                for m in methods {
                    prompt.push_str(&format!("\n  {}/{} ({})", m.service, m.method, m.kind));
                }
            }
            push_protocol_log(prompt, "Call log", log);
        }
    }
}

/// every loaded collection's tree, for the system prompt - so the model can
/// resolve "the Users collection" to an id itself instead of asking.
/// Collections the user ticked are listed first and marked as the focus.
/// request lines are capped at `MAX_PROMPT_REQUESTS`.
fn push_collections_overview(
    prompt: &mut String,
    collections: &[CollectionSummary],
    focused: &HashSet<usize>,
) {
    if collections.is_empty() {
        prompt.push_str("\n\nCollections: none yet.");
        return;
    }

    let mut ordered: Vec<&CollectionSummary> = collections.iter().collect();
    // stable: keeps the sidebar order within each group
    ordered.sort_by_key(|c| !focused.contains(&c.id));

    prompt.push_str(
        "\n\nCollections (every collection currently loaded, with its ids - use these, \
         never guess):",
    );
    let mut budget = MAX_PROMPT_REQUESTS;
    for col in ordered {
        let focus = if focused.contains(&col.id) {
            " [focused by the user]"
        } else {
            ""
        };
        prompt.push_str(&format!(
            "\n- collection #{} \"{}\"{focus}",
            col.id, col.name
        ));
        for folder in &col.folders {
            prompt.push_str(&format!("\n    folder: {}", folder.join("/")));
        }

        let shown = col.requests.len().min(budget);
        budget -= shown;
        for req in &col.requests[..shown] {
            let location = if req.folder_path.is_empty() {
                "collection root".to_string()
            } else {
                req.folder_path.join("/")
            };
            let kind = if req.protocol.is_empty() || req.protocol == "http" {
                req.method.clone()
            } else {
                req.protocol.to_uppercase()
            };
            prompt.push_str(&format!(
                "\n    request #{} \"{}\" ({kind} {}) in {location}",
                req.id, req.name, req.url
            ));
        }
        if shown < col.requests.len() {
            prompt.push_str(&format!(
                "\n    ... {} more requests not listed (ask the user to tick this collection \
                 under Context if you need them)",
                col.requests.len() - shown
            ));
        }
    }
}
