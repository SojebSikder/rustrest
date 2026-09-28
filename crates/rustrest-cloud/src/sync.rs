//! the sync engine. each cloud-backed collection keeps a `SyncState`: the
//! server's version of every item as of `seq` (the "base"). comparing the
//! local tree against the base tells us what we changed (→ push ops), and
//! comparing server changes against the base tells us what they changed
//! (→ merge). an item changed on both sides becomes a `PendingConflict`
//! that the user resolves; nothing is ever silently overwritten.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rustrest_core::collection::model::PostmanCollection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{CloudClient, CloudError};
use crate::convert::{self, TreeRow};
use crate::sort_key;
use crate::wire::{
    BatchResult, ChangeSet, CloudCollection, ConflictReason, ItemKind, ItemOp, NewItem, Snapshot,
    WireConflict, WireItem,
};

/// file (inside the collection's cache dir) holding its `SyncState`. dotfile, so the directory storage never touches it.
pub const STATE_FILE: &str = ".rustrest-cloud.json";

/// server's copy of one item, as of `SyncState::seq`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseItem {
    pub rev: i64,
    pub parent_uid: Option<String>,
    pub kind: ItemKind,
    pub sort_key: String,
    pub data: Value,
}

impl BaseItem {
    fn from_wire(item: &WireItem) -> Self {
        Self {
            rev: item.rev,
            parent_uid: item.parent_uid.clone(),
            kind: item.kind,
            sort_key: item.sort_key.clone(),
            data: convert::canonical_data(item.kind, &item.data),
        }
    }

    /// same content, ignoring rev
    fn same_as(&self, local: &LocalItem) -> bool {
        self.parent_uid == local.parent_uid
            && self.sort_key == local.sort_key
            && self.data == local.data
    }
}

/// an item both sides changed. `server` is their version (`None` = they deleted it),
/// ours is whatever is in the local tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConflict {
    pub server: Option<BaseItem>,
}

impl PendingConflict {
    /// a display name for the conflicted item, from the server's copy
    pub fn server_name(&self) -> Option<&str> {
        self.server.as_ref()?.data.get("name")?.as_str()
    }
}

/// server's version of the collection meta, when both sides changed it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaConflict {
    pub name: String,
    pub meta: Value,
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub server_url: String,
    pub team_id: String,
    pub collection_id: String,
    /// last change seq merged into the base
    pub seq: i64,
    /// server rev of the collection meta
    pub rev: i64,
    pub name: String,
    pub meta: Value,
    pub items: BTreeMap<String, BaseItem>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, PendingConflict>,
    #[serde(default)]
    pub meta_conflict: Option<MetaConflict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepMine,
    TakeTheirs,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// items updated locally from the server
    pub pulled: usize,
    /// items sent to the server
    pub pushed: usize,
    pub meta_pushed: bool,
    pub meta_pulled: bool,
    /// unresolved conflicts after this sync
    pub conflicts: usize,
    /// ops the server rejected as invalid (e.g. their parent was deleted);
    /// they're retried on the next sync, after a pull has repaired the tree
    pub rejected: Vec<String>,
}

impl SyncReport {
    /// true when this sync changed the local tree (so it needs saving/reloading)
    pub fn changed_local(&self) -> bool {
        self.pulled > 0 || self.meta_pulled
    }
}

// ---- local view ----

/// local item with the sort key it would have on the server
#[derive(Debug, Clone)]
struct LocalItem {
    parent_uid: Option<String>,
    kind: ItemKind,
    sort_key: String,
    data: Value,
    depth: usize,
}

struct LocalView {
    /// pre-order, so parents come before children
    order: Vec<String>,
    items: HashMap<String, LocalItem>,
}

impl LocalView {
    fn rows(&self) -> HashMap<String, TreeRow> {
        self.items
            .iter()
            .map(|(uid, item)| {
                (
                    uid.clone(),
                    row(
                        uid,
                        item.parent_uid.clone(),
                        item.kind,
                        &item.sort_key,
                        &item.data,
                    ),
                )
            })
            .collect()
    }
}

fn row(
    uid: &str,
    parent_uid: Option<String>,
    kind: ItemKind,
    sort_key: &str,
    data: &Value,
) -> TreeRow {
    TreeRow {
        uid: uid.to_string(),
        parent_uid,
        kind,
        sort_key: sort_key.to_string(),
        data: data.clone(),
    }
}

fn base_row(uid: &str, item: &BaseItem) -> TreeRow {
    row(
        uid,
        item.parent_uid.clone(),
        item.kind,
        &item.sort_key,
        &item.data,
    )
}

/// flattens the local tree and gives each item a sort key, reusing the base's key
/// wherever the item is still under the same parent in an order consistent with it.
fn local_view(collection: &PostmanCollection, state: &SyncState) -> LocalView {
    let (flat, children) = convert::flatten(collection);

    let mut keys: HashMap<String, String> = HashMap::new();
    for (parent, uids) in &children {
        let existing: Vec<Option<String>> = uids
            .iter()
            .map(|uid| {
                state
                    .items
                    .get(uid)
                    .filter(|b| &b.parent_uid == parent)
                    .map(|b| b.sort_key.clone())
            })
            .collect();
        for (uid, key) in uids.iter().zip(sort_key::assign(&existing)) {
            keys.insert(uid.clone(), key);
        }
    }

    let order = flat.iter().map(|f| f.uid.clone()).collect();
    let items = flat
        .into_iter()
        .map(|f| {
            let item = LocalItem {
                parent_uid: f.parent_uid,
                kind: f.kind,
                sort_key: keys.remove(&f.uid).unwrap_or_default(),
                data: f.data,
                depth: f.depth,
            };
            (f.uid, item)
        })
        .collect();
    LocalView { order, items }
}

fn locally_changed(base: Option<&BaseItem>, local: Option<&LocalItem>) -> bool {
    match (base, local) {
        (None, None) => false,
        (Some(b), Some(l)) => !b.same_as(l),
        _ => true, // created or deleted locally
    }
}

fn local_meta_changed(collection: &PostmanCollection, state: &SyncState) -> bool {
    collection.info.name != state.name || convert::collection_meta(collection) != state.meta
}

fn write_tree(
    collection: &mut PostmanCollection,
    rows: &HashMap<String, TreeRow>,
    next_id: &mut usize,
) {
    let rows: Vec<TreeRow> = rows.values().cloned().collect();
    collection.item = convert::rebuild(&rows, &collection.item, next_id);
}

impl SyncState {
    pub fn load(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dir.join(STATE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(STATE_FILE), json)
            .map_err(|e| format!("Failed to write {STATE_FILE}: {e}"))
    }

    fn from_snapshot(server_url: &str, snapshot: &Snapshot) -> Self {
        let col = &snapshot.collection;
        Self {
            server_url: server_url.to_string(),
            team_id: col.team_id.clone(),
            collection_id: col.id.clone(),
            seq: snapshot.seq,
            rev: col.rev,
            name: col.name.clone(),
            meta: convert::canonical_meta(&col.meta),
            items: snapshot
                .items
                .iter()
                .map(|i| (i.uid.clone(), BaseItem::from_wire(i)))
                .collect(),
            conflicts: BTreeMap::new(),
            meta_conflict: None,
        }
    }

    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty() || self.meta_conflict.is_some()
    }

    /// ops that would bring the server up to date with `collection` (which must have uids; see `convert::ensure_uids`).
    /// conflicted items are left out until resolved.
    pub fn pending_ops(&self, collection: &PostmanCollection) -> Vec<ItemOp> {
        let view = local_view(collection, self);
        self.ops_for(&view, &HashSet::new())
    }

    fn ops_for(&self, view: &LocalView, skip: &HashSet<String>) -> Vec<ItemOp> {
        let blocked = |uid: &str| self.conflicts.contains_key(uid) || skip.contains(uid);
        let mut creates = Vec::new();
        let mut updates: Vec<(usize, ItemOp)> = Vec::new();
        let mut deletes = Vec::new();

        for uid in &view.order {
            if blocked(uid) {
                continue;
            }
            let local = &view.items[uid];
            match self.items.get(uid) {
                None => creates.push(ItemOp::Create {
                    uid: uid.clone(),
                    parent_uid: local.parent_uid.clone(),
                    kind: local.kind,
                    sort_key: local.sort_key.clone(),
                    data: local.data.clone(),
                }),
                Some(base) if !base.same_as(local) => updates.push((
                    local.depth,
                    ItemOp::Update {
                        uid: uid.clone(),
                        base_rev: base.rev,
                        parent_uid: local.parent_uid.clone(),
                        sort_key: local.sort_key.clone(),
                        data: local.data.clone(),
                    },
                )),
                Some(_) => {}
            }
        }
        for (uid, base) in &self.items {
            if !view.items.contains_key(uid) && !blocked(uid) {
                deletes.push(ItemOp::Delete {
                    uid: uid.clone(),
                    base_rev: base.rev,
                });
            }
        }

        // creates run in pre-order (parents first). moves run shallowest
        // first: by then every item above has reached its final place, so a
        // move can never pass through a cycle (e.g. swapping a folder with
        // its child). deletes run last, after anything that was moved out
        // of a deleted folder has left it.
        updates.sort_by_key(|(depth, _)| *depth);
        creates
            .into_iter()
            .chain(updates.into_iter().map(|(_, op)| op))
            .chain(deletes)
            .collect()
    }

    /// merges server changes into the local tree and the base. returns how many local items changed.
    /// items changed on both sides become conflicts instead.
    pub fn apply_changes(
        &mut self,
        collection: &mut PostmanCollection,
        changes: &ChangeSet,
        next_id: &mut usize,
    ) -> (usize, bool) {
        convert::ensure_uids(collection);
        let view = local_view(collection, self);
        let mut rows = view.rows();
        let mut pulled = 0;

        for wire in &changes.items {
            let uid = &wire.uid;
            let server = wire.deleted_at.is_none().then(|| BaseItem::from_wire(wire));
            let base = self.items.get(uid);
            let local = view.items.get(uid);

            if let Some(conflict) = self.conflicts.get_mut(uid) {
                // still unresolved: just track their latest version
                conflict.server = server;
                continue;
            }

            if !locally_changed(base, local) {
                // we didn't touch it: take theirs
                match &server {
                    Some(s) => {
                        rows.insert(uid.clone(), base_row(uid, s));
                    }
                    None => {
                        rows.remove(uid);
                    }
                }
                if base != server.as_ref() {
                    pulled += 1;
                }
            } else {
                let converged = match (&server, local) {
                    (None, None) => true,
                    (Some(s), Some(l)) => s.same_as(l),
                    _ => false,
                };
                if !converged {
                    self.conflicts.insert(
                        uid.clone(),
                        PendingConflict {
                            server: server.clone(),
                        },
                    );
                    continue;
                }
            }

            match server {
                Some(s) => {
                    self.items.insert(uid.clone(), s);
                }
                None => {
                    self.items.remove(uid);
                }
            }
        }

        let mut meta_pulled = false;
        if let Some(col) = &changes.collection {
            meta_pulled = self.merge_meta(collection, col);
        }
        self.seq = changes.seq;

        if pulled > 0 {
            write_tree(collection, &rows, next_id);
        }
        (pulled, meta_pulled)
    }

    fn merge_meta(&mut self, collection: &mut PostmanCollection, col: &CloudCollection) -> bool {
        let server_meta = convert::canonical_meta(&col.meta);
        if col.rev <= self.rev && self.meta_conflict.is_none() {
            return false; // our own write coming back
        }
        if self.meta_conflict.is_some() || local_meta_changed(collection, self) {
            let converged = collection.info.name == col.name
                && convert::collection_meta(collection) == server_meta;
            if !converged {
                self.meta_conflict = Some(MetaConflict {
                    name: col.name.clone(),
                    meta: server_meta,
                    rev: col.rev,
                });
                return false;
            }
            self.meta_conflict = None;
            self.adopt_meta(col.name.clone(), server_meta, col.rev);
            return false;
        }
        convert::apply_meta(collection, &col.name, &server_meta);
        self.adopt_meta(col.name.clone(), server_meta, col.rev);
        true
    }

    fn adopt_meta(&mut self, name: String, meta: Value, rev: i64) {
        self.name = name;
        self.meta = meta;
        self.rev = rev;
    }

    /// settles a conflict on one item
    pub fn resolve(
        &mut self,
        collection: &mut PostmanCollection,
        uid: &str,
        resolution: Resolution,
        next_id: &mut usize,
    ) {
        let Some(conflict) = self.conflicts.remove(uid) else {
            return;
        };
        match resolution {
            // rebase ours onto theirs; the next push overwrites (or re-creates / deletes) it on the server
            Resolution::KeepMine => match conflict.server {
                Some(server) => {
                    self.items.insert(uid.to_string(), server);
                }
                None => {
                    self.items.remove(uid);
                }
            },
            Resolution::TakeTheirs => {
                let view = local_view(collection, self);
                let mut rows = view.rows();
                match &conflict.server {
                    Some(server) => {
                        rows.insert(uid.to_string(), base_row(uid, server));
                        self.items.insert(uid.to_string(), server.clone());
                    }
                    None => {
                        rows.remove(uid);
                        self.items.remove(uid);
                    }
                }
                write_tree(collection, &rows, next_id);
            }
        }
    }

    /// settles a conflict on the collection's name/meta
    pub fn resolve_meta(&mut self, collection: &mut PostmanCollection, resolution: Resolution) {
        let Some(conflict) = self.meta_conflict.take() else {
            return;
        };
        if resolution == Resolution::TakeTheirs {
            convert::apply_meta(collection, &conflict.name, &conflict.meta);
        }
        // either way we're now based on their version
        self.adopt_meta(conflict.name, conflict.meta, conflict.rev);
    }
}

// ---- network operations ----

/// uploads a local collection as a new cloud collection in `team_id`,
/// assigning uids first (so the caller must save the collection afterwards).
pub async fn upload(
    client: &CloudClient,
    team_id: &str,
    collection: &mut PostmanCollection,
) -> Result<SyncState, CloudError> {
    convert::ensure_uids(collection);
    let (flat, children) = convert::flatten(collection);
    let mut keys = HashMap::new();
    for uids in children.values() {
        for (uid, key) in uids.iter().zip(sort_key::evenly_spaced(uids.len())) {
            keys.insert(uid.clone(), key);
        }
    }
    let items: Vec<NewItem> = flat
        .into_iter()
        .map(|f| NewItem {
            sort_key: keys.remove(&f.uid).unwrap_or_default(),
            uid: f.uid,
            parent_uid: f.parent_uid,
            kind: f.kind,
            data: f.data,
        })
        .collect();
    let meta = convert::collection_meta(collection);

    let snapshot = client
        .create_collection(team_id, &collection.info.name, &meta, &items)
        .await?;
    Ok(SyncState::from_snapshot(client.base_url(), &snapshot))
}

/// downloads a cloud collection into a fresh local tree
pub async fn download(
    client: &CloudClient,
    collection_id: &str,
    next_id: &mut usize,
) -> Result<(PostmanCollection, SyncState), CloudError> {
    let snapshot = client.snapshot(collection_id).await?;
    let state = SyncState::from_snapshot(client.base_url(), &snapshot);

    let mut collection = PostmanCollection {
        id: 0,
        file_path: None,
        storage_dir: None,
        remote_dir: None,
        unsaved: false,
        info: rustrest_core::collection::model::CollectionInfo {
            name: state.name.clone(),
            postman_id: None,
            schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                .to_string(),
            description: None,
        },
        item: Vec::new(),
        variable: None,
        auth: None,
        event: None,
    };
    convert::apply_meta(&mut collection, &state.name, &state.meta);
    let rows: Vec<TreeRow> = state
        .items
        .iter()
        .map(|(uid, b)| base_row(uid, b))
        .collect();
    collection.item = convert::rebuild(&rows, &[], next_id);
    Ok((collection, state))
}

// ---- push, in steps ----
//
// a push is planned and recorded synchronously against the live tree, with
// only the network call in between. an app can therefore keep editing while
// a push is in flight: recording uses what was *sent*, so edits made
// meanwhile simply show up as new changes on the next round.

/// what a push will send
#[derive(Debug, Clone, Default)]
pub struct PushPlan {
    pub ops: Vec<ItemOp>,
    /// `(base_rev, name, meta)` when the collection meta changed
    pub meta: Option<(i64, String, Value)>,
}

impl PushPlan {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.meta.is_none()
    }
}

/// what the server said to a `PushPlan`
#[derive(Debug, Clone)]
pub struct PushOutcome {
    pub batch: Option<Result<BatchResult, CloudError>>,
    pub meta: Option<Result<CloudCollection, CloudError>>,
}

impl SyncState {
    /// plans a push of everything changed locally. ops for conflicted items
    /// and for `skip` (ops the server rejected as invalid this round) are
    /// left out.
    pub fn plan_push(&self, collection: &PostmanCollection, skip: &HashSet<String>) -> PushPlan {
        let view = local_view(collection, self);
        let meta =
            (self.meta_conflict.is_none() && local_meta_changed(collection, self)).then(|| {
                (
                    self.rev,
                    collection.info.name.clone(),
                    convert::collection_meta(collection),
                )
            });
        PushPlan {
            ops: self.ops_for(&view, skip),
            meta,
        }
    }

    /// records the server's answer to `plan`. returns true when the batch
    /// was rejected over conflicts, so the (now smaller) rest should be
    /// planned and sent again. invalid ops are added to `skip`.
    pub fn record_push(
        &mut self,
        plan: &PushPlan,
        outcome: PushOutcome,
        skip: &mut HashSet<String>,
        report: &mut SyncReport,
    ) -> Result<bool, CloudError> {
        let mut retry = false;
        match outcome.batch {
            None => {}
            Some(Ok(result)) => {
                self.record_applied(&plan.ops, &result);
                report.pushed += plan.ops.len();
            }
            Some(Err(CloudError::Conflicts(conflicts))) => {
                self.record_conflicts(&plan.ops, conflicts, skip, report);
                retry = true;
            }
            Some(Err(err)) => return Err(err),
        }

        match outcome.meta {
            None => {}
            Some(Ok(col)) => {
                if let Some((_, _, meta)) = &plan.meta {
                    self.adopt_meta(col.name, meta.clone(), col.rev);
                    report.meta_pushed = true;
                }
            }
            Some(Err(CloudError::MetaConflict(current))) => {
                self.meta_conflict = Some(MetaConflict {
                    name: current.name,
                    meta: convert::canonical_meta(&current.meta),
                    rev: current.rev,
                });
            }
            Some(Err(err)) => return Err(err),
        }
        report.conflicts = self.conflicts.len() + usize::from(self.meta_conflict.is_some());
        Ok(retry)
    }

    fn record_applied(&mut self, ops: &[ItemOp], result: &BatchResult) {
        for (op, res) in ops.iter().zip(&result.results) {
            match op {
                ItemOp::Create {
                    uid,
                    parent_uid,
                    kind,
                    sort_key,
                    data,
                } => {
                    self.items.insert(
                        uid.clone(),
                        BaseItem {
                            rev: res.rev,
                            parent_uid: parent_uid.clone(),
                            kind: *kind,
                            sort_key: sort_key.clone(),
                            data: data.clone(),
                        },
                    );
                }
                ItemOp::Update {
                    uid,
                    parent_uid,
                    sort_key,
                    data,
                    ..
                } => {
                    if let Some(base) = self.items.get_mut(uid) {
                        base.rev = res.rev;
                        base.parent_uid = parent_uid.clone();
                        base.sort_key = sort_key.clone();
                        base.data = data.clone();
                    }
                }
                ItemOp::Delete { uid, .. } => {
                    self.items.remove(uid);
                    for gone in &res.deleted {
                        self.items.remove(gone);
                    }
                }
            }
        }
    }

    fn record_conflicts(
        &mut self,
        ops: &[ItemOp],
        conflicts: Vec<WireConflict>,
        skip: &mut HashSet<String>,
        report: &mut SyncReport,
    ) {
        for c in conflicts {
            let Some(op) = ops.get(c.index) else { continue };
            match c.reason {
                ConflictReason::Stale | ConflictReason::Deleted | ConflictReason::Exists => {
                    let server = c
                        .current
                        .filter(|cur| cur.deleted_at.is_none())
                        .map(|cur| BaseItem::from_wire(&cur));
                    self.conflicts
                        .insert(c.uid.clone(), PendingConflict { server });
                }
                ConflictReason::Missing => {
                    // the server never had it: an update becomes a create on
                    // the next round, a delete is moot
                    self.items.remove(op.uid());
                }
                ConflictReason::Invalid => {
                    report.rejected.push(format!("{}: {}", c.uid, c.message));
                    skip.insert(c.uid.clone());
                }
            }
        }
    }
}

/// sends a plan. the meta update is only attempted if the batch went
/// through (or there was none), so a rejected batch is retried as a whole.
pub async fn send_push(client: &CloudClient, collection_id: &str, plan: &PushPlan) -> PushOutcome {
    let batch = if plan.ops.is_empty() {
        None
    } else {
        Some(client.batch(collection_id, &plan.ops).await)
    };
    let batch_ok = batch.as_ref().is_none_or(Result::is_ok);
    let meta = match &plan.meta {
        Some((base_rev, name, meta)) if batch_ok => Some(
            client
                .update_collection(collection_id, *base_rev, name, meta)
                .await,
        ),
        _ => None,
    };
    PushOutcome { batch, meta }
}

/// pulls server changes into `collection`
pub async fn pull(
    client: &CloudClient,
    state: &mut SyncState,
    collection: &mut PostmanCollection,
    next_id: &mut usize,
) -> Result<(usize, bool), CloudError> {
    let changes = client.changes(&state.collection_id, state.seq).await?;
    Ok(state.apply_changes(collection, &changes, next_id))
}

/// pushes local changes without pulling first, retrying around conflicts
pub async fn push(
    client: &CloudClient,
    state: &mut SyncState,
    collection: &PostmanCollection,
    report: &mut SyncReport,
) -> Result<(), CloudError> {
    let mut skip = HashSet::new();
    // a rejected batch shrinks by at least one op per round
    for _ in 0..4 {
        let plan = state.plan_push(collection, &skip);
        if plan.is_empty() {
            break;
        }
        let outcome = send_push(client, &state.collection_id, &plan).await;
        if !state.record_push(&plan, outcome, &mut skip, report)? {
            break;
        }
    }
    report.conflicts = state.conflicts.len() + usize::from(state.meta_conflict.is_some());
    Ok(())
}

/// full sync: pull, push our changes, pull again to catch up with anything that landed meanwhile.
/// conflicts are recorded in `state`, never overwritten. resolve them and sync again.
pub async fn sync(
    client: &CloudClient,
    state: &mut SyncState,
    collection: &mut PostmanCollection,
    next_id: &mut usize,
) -> Result<SyncReport, CloudError> {
    let mut report = SyncReport::default();
    convert::ensure_uids(collection);
    let (pulled, meta_pulled) = pull(client, state, collection, next_id).await?;
    report.pulled += pulled;
    report.meta_pulled |= meta_pulled;

    push(client, state, collection, &mut report).await?;

    let (pulled, meta_pulled) = pull(client, state, collection, next_id).await?;
    report.pulled += pulled;
    report.meta_pulled |= meta_pulled;
    report.conflicts = state.conflicts.len() + usize::from(state.meta_conflict.is_some());
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::tests::{collection, folder, request};
    use crate::wire::OpResult;
    use rustrest_core::collection::model::{CollectionItem, PostmanRequestNode};

    /// a state whose base is exactly `col`, as if just uploaded
    fn synced(col: &mut PostmanCollection) -> SyncState {
        convert::ensure_uids(col);
        let mut state = SyncState {
            server_url: String::new(),
            team_id: "t".into(),
            collection_id: "c".into(),
            seq: 1,
            rev: 1,
            name: col.info.name.clone(),
            meta: convert::collection_meta(col),
            items: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            meta_conflict: None,
        };
        let plan = state.plan_push(col, &HashSet::new());
        let result = ok_result(&plan);
        state.record_applied(&plan.ops, &result);
        state
    }

    fn ok_result(plan: &PushPlan) -> BatchResult {
        BatchResult {
            results: plan
                .ops
                .iter()
                .map(|op| OpResult {
                    op: String::new(),
                    uid: op.uid().to_string(),
                    rev: 2,
                    deleted: Vec::new(),
                })
                .collect(),
            seq: 2,
        }
    }

    fn req_mut<'a>(items: &'a mut [CollectionItem], name: &str) -> &'a mut PostmanRequestNode {
        items
            .iter_mut()
            .find_map(|i| match i {
                CollectionItem::Request(r) if r.name == name => Some(r),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn clean_state_has_nothing_to_push() {
        let mut col = collection(vec![request(1, "a"), folder("f", vec![request(2, "b")])]);
        let state = synced(&mut col);
        assert!(state.plan_push(&col, &HashSet::new()).is_empty());
    }

    #[test]
    fn edits_made_while_a_push_is_in_flight_are_not_lost() {
        let mut col = collection(vec![request(1, "a"), request(2, "b")]);
        let mut state = synced(&mut col);

        req_mut(&mut col.item, "a").request.method = "POST".into();
        let plan = state.plan_push(&col, &HashSet::new());
        assert_eq!(plan.ops.len(), 1);

        // the user keeps editing before the server answers
        req_mut(&mut col.item, "a").request.method = "PUT".into();
        req_mut(&mut col.item, "b").request.method = "DELETE".into();

        let outcome = PushOutcome {
            batch: Some(Ok(ok_result(&plan))),
            meta: None,
        };
        let mut report = SyncReport::default();
        state
            .record_push(&plan, outcome, &mut HashSet::new(), &mut report)
            .unwrap();

        // both later edits are still pending, against the new rev
        let next = state.plan_push(&col, &HashSet::new());
        assert_eq!(next.ops.len(), 2, "{:?}", next.ops);
        assert!(
            next.ops
                .iter()
                .all(|op| matches!(op, ItemOp::Update { base_rev: 2, .. }))
        );
    }

    #[test]
    fn moves_are_ordered_shallowest_first_and_deletes_last() {
        // outer/inner/x  ->  inner/outer/x   and delete y
        let mut col = collection(vec![
            folder("outer", vec![folder("inner", vec![request(1, "x")])]),
            request(2, "y"),
        ]);
        let state = synced(&mut col);

        let CollectionItem::Folder(mut outer) = col.item.remove(0) else {
            unreachable!()
        };
        let CollectionItem::Folder(mut inner) = outer.item.remove(0) else {
            unreachable!()
        };
        let x = inner.item.remove(0);
        outer.item.push(x);
        inner.item.push(CollectionItem::Folder(outer));
        col.item = vec![CollectionItem::Folder(inner)]; // y deleted

        let ops = state.plan_push(&col, &HashSet::new()).ops;
        let kinds: Vec<&str> = ops
            .iter()
            .map(|op| match op {
                ItemOp::Create { .. } => "create",
                ItemOp::Update { .. } => "update",
                ItemOp::Delete { .. } => "delete",
            })
            .collect();
        assert_eq!(kinds.last(), Some(&"delete"));
        // "inner" (moving to the root) must precede "outer" (moving under inner)
        let pos = |name: &str| {
            ops.iter()
                .position(|op| matches!(op, ItemOp::Update { data, .. } if data["name"] == name))
                .unwrap()
        };
        assert!(pos("inner") < pos("outer"));
    }

    #[test]
    fn server_change_to_an_untouched_item_is_applied_locally() {
        let mut col = collection(vec![request(1, "a"), request(2, "b")]);
        let mut state = synced(&mut col);
        let uid = state.items.keys().next().unwrap().clone();
        let mut theirs = state.items[&uid].clone();
        theirs.data["request"]["method"] = "PATCH".into();

        let changes = ChangeSet {
            collection: None,
            items: vec![WireItem {
                uid: uid.clone(),
                parent_uid: theirs.parent_uid.clone(),
                kind: theirs.kind,
                sort_key: theirs.sort_key.clone(),
                data: theirs.data.clone(),
                rev: 3,
                seq: 3,
                deleted_at: None,
            }],
            seq: 3,
        };
        let ids_before: Vec<usize> = col
            .item
            .iter()
            .map(|i| match i {
                CollectionItem::Request(r) => r.id,
                _ => 0,
            })
            .collect();
        let mut next_id = 50;
        let (pulled, _) = state.apply_changes(&mut col, &changes, &mut next_id);
        assert_eq!(pulled, 1);
        assert!(state.conflicts.is_empty());
        assert!(state.plan_push(&col, &HashSet::new()).is_empty());
        let ids_after: Vec<usize> = col
            .item
            .iter()
            .map(|i| match i {
                CollectionItem::Request(r) => r.id,
                _ => 0,
            })
            .collect();
        assert_eq!(ids_before, ids_after, "open tabs stay attached");
        assert_eq!(next_id, 50);
    }

    #[test]
    fn concurrent_edit_becomes_a_conflict_and_both_resolutions_work() {
        for resolution in [Resolution::KeepMine, Resolution::TakeTheirs] {
            let mut col = collection(vec![request(1, "a")]);
            let mut state = synced(&mut col);
            let uid = state.items.keys().next().unwrap().clone();

            req_mut(&mut col.item, "a").request.method = "MINE".into();
            let mut theirs = state.items[&uid].clone();
            theirs.data["request"]["method"] = "THEIRS".into();
            let changes = ChangeSet {
                collection: None,
                items: vec![WireItem {
                    uid: uid.clone(),
                    parent_uid: None,
                    kind: theirs.kind,
                    sort_key: theirs.sort_key.clone(),
                    data: theirs.data.clone(),
                    rev: 3,
                    seq: 3,
                    deleted_at: None,
                }],
                seq: 3,
            };
            let mut next_id = 50;
            state.apply_changes(&mut col, &changes, &mut next_id);
            assert_eq!(state.conflicts.len(), 1);
            assert_eq!(
                req_mut(&mut col.item, "a").request.method,
                "MINE",
                "never overwritten"
            );
            assert!(
                state.plan_push(&col, &HashSet::new()).ops.is_empty(),
                "conflicts aren't pushed"
            );

            state.resolve(&mut col, &uid, resolution, &mut next_id);
            let ops = state.plan_push(&col, &HashSet::new()).ops;
            match resolution {
                Resolution::KeepMine => {
                    assert!(
                        matches!(&ops[..], [ItemOp::Update { base_rev: 3, .. }]),
                        "{ops:?}"
                    );
                }
                Resolution::TakeTheirs => {
                    assert!(ops.is_empty());
                    assert_eq!(req_mut(&mut col.item, "a").request.method, "THEIRS");
                }
            }
        }
    }
}
