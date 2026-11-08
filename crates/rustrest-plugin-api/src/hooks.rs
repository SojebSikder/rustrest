use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An outgoing request, threaded through every enabled plugin with the
/// `RequestHooks` capability before it is sent. Mirrors the shape of the
/// existing `pm.*` JS pre-request scripting context so behavior stays
/// consistent between the two mechanisms.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestContext {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
}

/// A received response, threaded through every enabled plugin with the
/// `RequestHooks` capability right after it arrives, mirroring the existing
/// `pm.response`/post-response test-script context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseContext {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub test_results: Vec<TestResult>,
}

/// a request living somewhere in a `CollectionSummary`'s tree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestSummary {
    pub id: usize,
    pub name: String,
    pub folder_path: Vec<String>,
    pub method: String,
    /// the request's saved URL (a gRPC request's endpoint)
    #[serde(default)]
    pub url: String,
    /// "http", "websocket", "graphql" or "grpc"
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "http".to_string()
}

/// one line of a WebSocket/GraphQL-subscription/gRPC tab's message log.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtocolLogEntry {
    /// "out" (sent by the user) or "in" (received)
    pub direction: String,
    pub label: String,
    pub body: String,
}

/// a gRPC method the active gRPC tab's target exposes, once discovered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrpcMethodSummary {
    pub service: String,
    pub method: String,
    /// "unary", "server_streaming", "client_streaming" or "bidi_streaming"
    pub kind: String,
}

/// the active tab when it's a WebSocket, GraphQL or gRPC request rather
/// than a plain HTTP one - handed to a `RightPanel` plugin as
/// `RightPanelContext::active_protocol_request`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolRequestContext {
    WebSocket {
        url: String,
        headers: Vec<(String, String)>,
        /// the message currently typed in the compose box
        message: String,
        connected: bool,
        /// most recent log lines, oldest first
        log: Vec<ProtocolLogEntry>,
    },
    GraphQl {
        url: String,
        headers: Vec<(String, String)>,
        query: String,
        variables: String,
        operation_name: String,
        /// the last response body (or error text)
        response: Option<String>,
        /// most recent subscription events, oldest first
        log: Vec<ProtocolLogEntry>,
    },
    Grpc {
        endpoint: String,
        use_tls: bool,
        metadata: Vec<(String, String)>,
        service: Option<String>,
        method: Option<String>,
        request_json: String,
        /// every method the target exposes, empty until discovered
        methods: Vec<GrpcMethodSummary>,
        /// most recent log lines, oldest first
        log: Vec<ProtocolLogEntry>,
    },
}

/// a read-only snapshot of one loaded collection's tree, handed to a
/// `RightPanel` plugin so it can reference existing collections/folders/
/// requests by id/name (e.g. to let an AI assistant propose a
/// `CollectionOperation` against something that already exists instead of
/// guessing ids).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: usize,
    pub name: String,
    /// every folder in the tree, as its full path (e.g. `["Auth", "Login"]`
    /// for a folder named "Login" nested under "Auth").
    pub folders: Vec<Vec<String>>,
    pub requests: Vec<RequestSummary>,
}

/// a read-only snapshot of one environment (name + its active variables),
/// handed to a `RightPanel` plugin so it can offer environment variables as
/// optional context. Variable *values* are included, which may be secrets
/// a plugin surfacing these to a user-selectable list should say so in its UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvSummary {
    pub name: String,
    pub variables: Vec<(String, String)>,
}

/// ambient context handed to a `RightPanel` plugin on every render/event -
/// a snapshot of whatever the active tab currently holds, so a docked panel
/// (e.g. an AI assistant) can act on "the current request" without needing
/// its own request-tracking plumbing. Both fields are `None` when the active
/// tab isn't an HTTP request tab (or there is no active tab).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RightPanelContext {
    pub active_request: Option<RequestContext>,
    pub active_response: Option<ResponseContext>,
    /// every collection currently loaded, so a plugin can act on the
    /// collection tree itself (not just the active request).
    #[serde(default)]
    pub collections: Vec<CollectionSummary>,
    /// every environment currently defined.
    #[serde(default)]
    pub environments: Vec<EnvSummary>,
    /// set instead of `active_request` when the active tab is a WebSocket,
    /// GraphQL or gRPC request.
    #[serde(default)]
    pub active_protocol_request: Option<ProtocolRequestContext>,
}

/// a set of edits a `RightPanel` plugin wants applied to the active request
/// tab. Every field is optional - only the fields a plugin actually wants to
/// change need be `Some`; everything else is left as-is.
///
/// On a WebSocket/GraphQL/gRPC request, `url` is its URL (a gRPC request's
/// endpoint), `headers` its headers (gRPC metadata), and `body` the
/// WebSocket message to send or the gRPC request JSON. Fields that don't
/// apply to the request's protocol are ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestPatch {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub body: Option<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    /// GraphQL query document
    #[serde(default)]
    pub query: Option<String>,
    /// GraphQL variables, as a JSON string
    #[serde(default)]
    pub variables: Option<String>,
    /// GraphQL operation name
    #[serde(default)]
    pub operation_name: Option<String>,
    /// gRPC service name, e.g. "package.Greeter"
    #[serde(default)]
    pub grpc_service: Option<String>,
    /// gRPC method name, e.g. "SayHello"
    #[serde(default)]
    pub grpc_method: Option<String>,
    /// gRPC: connect over TLS
    #[serde(default)]
    pub use_tls: Option<bool>,
}

impl RequestPatch {
    /// names of the fields this patch would change, e.g. `["url", "body"]`.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("method", self.method.is_some()),
            ("url", self.url.is_some()),
            ("headers", self.headers.is_some()),
            ("body", self.body.is_some()),
            ("pre-request script", self.pre_request_script.is_some()),
            ("post-response script", self.post_response_script.is_some()),
            ("query", self.query.is_some()),
            ("variables", self.variables.is_some()),
            ("operation name", self.operation_name.is_some()),
            ("gRPC service", self.grpc_service.is_some()),
            ("gRPC method", self.grpc_method.is_some()),
            ("TLS", self.use_tls.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }

    /// true if the patch wouldn't change anything.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }
}

/// returned by `Plugin::on_right_panel_event` to tell the host what changed:
/// the panel's own UI, the active request tab, or both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RightPanelAction {
    /// nothing changed; leave the currently rendered tree and tab as-is.
    None,
    /// re-render the panel with this tree; don't touch the active tab.
    UpdateUi(crate::ui::UiNode),
    /// apply this patch to the active request tab; don't re-render the panel.
    ApplyPatch(RequestPatch),
    /// re-render the panel with this tree AND apply this patch to the active
    /// request tab.
    UpdateUiAndApplyPatch(crate::ui::UiNode, RequestPatch),
    /// re-render the panel with this tree AND ask the host to perform a
    /// collection-tree operation (create/rename/delete/duplicate/move a
    /// collection, folder, or request). Destructive operations are confirmed
    /// with the user before the host applies them.
    UpdateUiAndProposeCollectionOp(crate::ui::UiNode, CollectionOperation),
    /// like `UpdateUiAndProposeCollectionOp`, but for several operations
    /// applied in order as one batch (e.g. create a folder, then requests in it).
    UpdateUiAndProposeCollectionOps(crate::ui::UiNode, Vec<CollectionOperation>),
}

/// a create/rename/delete/duplicate/move performed on the collection tree
/// itself, proposed by a `RightPanel` plugin and executed host-side.
/// Internally tagged so it's easy
/// for an LLM to produce as e.g. `{"op":"create_folder","collection_id":1,...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CollectionOperation {
    CreateCollection {
        name: String,
    },
    RenameCollection {
        collection_id: usize,
        new_name: String,
    },
    DeleteCollection {
        collection_id: usize,
    },
    CreateFolder {
        collection_id: usize,
        parent_path: Vec<String>,
        name: String,
    },
    RenameFolder {
        collection_id: usize,
        path: Vec<String>,
        new_name: String,
    },
    DeleteFolder {
        collection_id: usize,
        path: Vec<String>,
    },
    CreateRequest {
        collection_id: usize,
        parent_path: Vec<String>,
        name: String,
        /// HTTP method; ignored for non-HTTP protocols
        #[serde(default)]
        method: String,
        /// URL, or a gRPC request's endpoint
        url: String,
        /// "http" (default), "websocket", "graphql" or "grpc"
        #[serde(default)]
        protocol: Option<String>,
    },
    RenameRequest {
        collection_id: usize,
        request_id: usize,
        new_name: String,
    },
    DeleteRequest {
        collection_id: usize,
        parent_path: Vec<String>,
        request_id: usize,
    },
    DuplicateRequest {
        collection_id: usize,
        parent_path: Vec<String>,
        request_id: usize,
    },
    MoveRequest {
        collection_id: usize,
        from_path: Vec<String>,
        request_id: usize,
        to_path: Vec<String>,
    },
    /// edits a saved request's contents in place. Only the fields set in
    /// `patch` change, `headers`, if set, replaces the whole header list.
    UpdateRequest {
        collection_id: usize,
        request_id: usize,
        #[serde(flatten)]
        patch: RequestPatch,
    },
}

impl CollectionOperation {
    /// true for operations that remove or relocate something, which the host
    /// confirms with the user before applying rather than acting immediately.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::DeleteCollection { .. }
                | Self::DeleteFolder { .. }
                | Self::DeleteRequest { .. }
                | Self::MoveRequest { .. }
                | Self::UpdateRequest { .. }
        )
    }

    /// human-readable one-liner describing this operation, used both as the
    /// confirm-dialog message and as a log/toast line.
    pub fn describe(&self) -> String {
        match self {
            Self::CreateCollection { name } => format!("Create collection \"{name}\""),
            Self::RenameCollection {
                collection_id,
                new_name,
            } => format!("Rename collection #{collection_id} to \"{new_name}\""),
            Self::DeleteCollection { collection_id } => {
                format!("Delete collection #{collection_id}")
            }
            Self::CreateFolder {
                parent_path, name, ..
            } => {
                if parent_path.is_empty() {
                    format!("Create folder \"{name}\"")
                } else {
                    format!("Create folder \"{name}\" in {}", parent_path.join("/"))
                }
            }
            Self::RenameFolder { path, new_name, .. } => {
                format!("Rename folder {} to \"{new_name}\"", path.join("/"))
            }
            Self::DeleteFolder { path, .. } => format!("Delete folder {}", path.join("/")),
            Self::CreateRequest {
                parent_path,
                name,
                method,
                url,
                protocol,
                ..
            } => {
                let kind = match protocol.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    None | Some("" | "http") => method.to_string(),
                    Some(other) => other.to_ascii_uppercase(),
                };
                if parent_path.is_empty() {
                    format!("Create request \"{name}\" ({kind} {url})")
                } else {
                    format!(
                        "Create request \"{name}\" ({kind} {url}) in {}",
                        parent_path.join("/")
                    )
                }
            }
            Self::RenameRequest {
                request_id,
                new_name,
                ..
            } => format!("Rename request #{request_id} to \"{new_name}\""),
            Self::DeleteRequest { request_id, .. } => format!("Delete request #{request_id}"),
            Self::DuplicateRequest { request_id, .. } => {
                format!("Duplicate request #{request_id}")
            }
            Self::MoveRequest {
                request_id,
                to_path,
                ..
            } => {
                if to_path.is_empty() {
                    format!("Move request #{request_id} to the collection root")
                } else {
                    format!("Move request #{request_id} to {}", to_path.join("/"))
                }
            }
            Self::UpdateRequest {
                request_id, patch, ..
            } => {
                let fields = patch.changed_fields();
                if fields.is_empty() {
                    format!("Update request #{request_id}")
                } else {
                    format!("Update request #{request_id} ({})", fields.join(", "))
                }
            }
        }
    }
}
