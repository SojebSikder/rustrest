//! request/response shapes of the cloud API, in the `{"success","message","data"}` envelope.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct Envelope<T> {
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

impl Role {
    pub fn can_edit(self) -> bool {
        matches!(self, Role::Owner | Role::Editor)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub role: Role,
    #[serde(default)]
    pub is_personal: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamMember {
    pub user_id: String,
    pub role: Role,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudCollection {
    pub id: String,
    pub team_id: String,
    pub name: String,
    #[serde(default)]
    pub meta: Value,
    pub rev: i64,
    pub seq: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Folder,
    Request,
}

/// folder/request as stored on the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireItem {
    pub uid: String,
    pub parent_uid: Option<String>,
    pub kind: ItemKind,
    #[serde(default)]
    pub sort_key: String,
    pub data: Value,
    pub rev: i64,
    #[serde(default)]
    pub seq: i64,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub collection: CloudCollection,
    pub items: Vec<WireItem>,
    pub seq: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeSet {
    /// present only when the collection's name/meta changed
    pub collection: Option<CloudCollection>,
    pub items: Vec<WireItem>,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewItem {
    pub uid: String,
    pub parent_uid: Option<String>,
    pub kind: ItemKind,
    pub sort_key: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CreateCollection<'a> {
    pub name: &'a str,
    pub meta: &'a Value,
    pub items: &'a [NewItem],
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateCollection<'a> {
    pub base_rev: i64,
    pub name: &'a str,
    pub meta: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum ItemOp {
    Create {
        uid: String,
        parent_uid: Option<String>,
        kind: ItemKind,
        sort_key: String,
        data: Value,
    },
    Update {
        uid: String,
        base_rev: i64,
        parent_uid: Option<String>,
        sort_key: String,
        data: Value,
    },
    Delete {
        uid: String,
        base_rev: i64,
    },
}

impl ItemOp {
    pub fn uid(&self) -> &str {
        match self {
            ItemOp::Create { uid, .. }
            | ItemOp::Update { uid, .. }
            | ItemOp::Delete { uid, .. } => uid,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Batch<'a> {
    pub ops: &'a [ItemOp],
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpResult {
    pub op: String,
    pub uid: String,
    /// 0 when the op was a no-op (e.g. deleting something already deleted)
    pub rev: i64,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchResult {
    pub results: Vec<OpResult>,
    pub seq: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictReason {
    Stale,
    Deleted,
    Missing,
    Exists,
    Invalid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireConflict {
    pub index: usize,
    pub uid: String,
    pub reason: ConflictReason,
    #[serde(default)]
    pub message: String,
    /// the server's copy, when it has one
    pub current: Option<WireItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ConflictBody {
    pub conflicts: Vec<WireConflict>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryEntry {
    pub seq: i64,
    pub op: String,
    pub rev: i64,
    pub item_uid: Option<String>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudEnvironment {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub data: Value,
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EnvironmentBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<&'a str>,
    pub name: &'a str,
    pub data: &'a Value,
    pub base_rev: i64,
}
