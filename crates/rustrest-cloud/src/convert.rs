//! converts between the app's nested `PostmanCollection` tree and the flat
//! `(uid, parent_uid, sort_key, data)` rows the server stores.

use std::collections::{HashMap, HashSet};

use rustrest_core::collection::model::{
    CollectionItem, PostmanCollection, PostmanEvent, PostmanFolder, PostmanProtocolProfileBehavior,
    PostmanRequestNode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::wire::ItemKind;

/// a tree item flattened for sync. `data` is the item without its children
/// or uid, in canonical JSON form.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatItem {
    pub uid: String,
    pub parent_uid: Option<String>,
    pub kind: ItemKind,
    pub data: Value,
    /// depth in the tree (0 = collection root), used to order moves safely
    pub depth: usize,
}

/// the folder fields that sync (children are rows of their own)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FolderData {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event: Option<Vec<PostmanEvent>>,
    #[serde(
        rename = "protocolProfileBehavior",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    protocol_profile_behavior: Option<PostmanProtocolProfileBehavior>,
}

/// collection-level fields that sync as the collection's `meta`
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaData {
    info: rustrest_core::collection::model::CollectionInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    variable: Option<Vec<rustrest_core::collection::model::PostmanVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<rustrest_core::RequestAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event: Option<Vec<PostmanEvent>>,
}

pub fn new_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// gives every item a valid, unique uid (keeping existing ones where possible).
/// returns true if anything changed, meaning the collection needs saving so the uids persist.
pub fn ensure_uids(collection: &mut PostmanCollection) -> bool {
    fn walk(items: &mut [CollectionItem], seen: &mut HashSet<String>, changed: &mut bool) {
        for item in items {
            let uid = match item {
                CollectionItem::Request(r) => &mut r.uid,
                CollectionItem::Folder(f) => &mut f.uid,
            };
            // postman exports may carry non-uuid ids; the server needs uuids
            let keep = uid
                .as_deref()
                .is_some_and(|u| uuid::Uuid::parse_str(u).is_ok() && !seen.contains(u));
            if !keep {
                *uid = Some(new_uid());
                *changed = true;
            }
            seen.insert(uid.clone().unwrap());
            if let CollectionItem::Folder(f) = item {
                walk(&mut f.item, seen, changed);
            }
        }
    }
    let mut changed = false;
    walk(&mut collection.item, &mut HashSet::new(), &mut changed);
    changed
}

/// the collection's synced meta (info, variables, auth, scripts)
pub fn collection_meta(collection: &PostmanCollection) -> Value {
    serde_json::to_value(MetaData {
        info: collection.info.clone(),
        variable: collection.variable.clone(),
        auth: collection.auth.clone(),
        event: collection.event.clone(),
    })
    .unwrap_or(Value::Null)
}

/// applies server meta onto the collection, leaving local-only fields alone
pub fn apply_meta(collection: &mut PostmanCollection, name: &str, meta: &Value) {
    if let Ok(meta) = serde_json::from_value::<MetaData>(meta.clone()) {
        collection.info = meta.info;
        collection.variable = meta.variable;
        collection.auth = meta.auth;
        collection.event = meta.event;
    }
    collection.info.name = name.to_string();
}

/// round-trips a server payload through the app's model, so that what we
/// compare against later is exactly what the app would serialize itself.
/// payloads the model can't read are kept as is.
pub fn canonical_data(kind: ItemKind, data: &Value) -> Value {
    match kind {
        ItemKind::Request => serde_json::from_value::<PostmanRequestNode>(data.clone())
            .ok()
            .map(|node| request_data(&node)),
        ItemKind::Folder => serde_json::from_value::<FolderData>(data.clone())
            .ok()
            .and_then(|f| serde_json::to_value(f).ok()),
    }
    .unwrap_or_else(|| data.clone())
}

pub fn canonical_meta(meta: &Value) -> Value {
    serde_json::from_value::<MetaData>(meta.clone())
        .ok()
        .and_then(|m| serde_json::to_value(m).ok())
        .unwrap_or_else(|| meta.clone())
}

fn request_data(node: &PostmanRequestNode) -> Value {
    let mut value = serde_json::to_value(node).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.remove("id"); // uid travels separately
    }
    value
}

fn folder_data(folder: &PostmanFolder) -> Value {
    serde_json::to_value(FolderData {
        name: folder.name.clone(),
        description: folder.description.clone(),
        event: folder.event.clone(),
        protocol_profile_behavior: folder.protocol_profile_behavior.clone(),
    })
    .unwrap_or(Value::Null)
}

/// flattens the tree in pre-order (parents before children). every item
/// must already have a uid (`ensure_uids`). returns items grouped as
/// `(parent_uid -> ordered child uids)` alongside.
pub fn flatten(
    collection: &PostmanCollection,
) -> (Vec<FlatItem>, HashMap<Option<String>, Vec<String>>) {
    fn walk(
        items: &[CollectionItem],
        parent: Option<&str>,
        depth: usize,
        out: &mut Vec<FlatItem>,
        children: &mut HashMap<Option<String>, Vec<String>>,
    ) {
        for item in items {
            let (uid, kind, data) = match item {
                CollectionItem::Request(r) => (r.uid.clone(), ItemKind::Request, request_data(r)),
                CollectionItem::Folder(f) => (f.uid.clone(), ItemKind::Folder, folder_data(f)),
            };
            let uid = uid.expect("flatten called before ensure_uids");
            children
                .entry(parent.map(str::to_string))
                .or_default()
                .push(uid.clone());
            out.push(FlatItem {
                uid: uid.clone(),
                parent_uid: parent.map(str::to_string),
                kind,
                data,
                depth,
            });
            if let CollectionItem::Folder(f) = item {
                walk(&f.item, Some(&uid), depth + 1, out, children);
            }
        }
    }
    let mut out = Vec::new();
    let mut children = HashMap::new();
    walk(&collection.item, None, 0, &mut out, &mut children);
    (out, children)
}

/// one row to rebuild the tree from
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub uid: String,
    pub parent_uid: Option<String>,
    pub kind: ItemKind,
    pub sort_key: String,
    pub data: Value,
}

/// rebuilds the nested tree from flat rows, ordering siblings by sort key
/// (then uid, for a stable order on ties). rows whose parent is missing are
/// kept at the root rather than dropped. request `id`s are carried over from
/// `previous` by uid (so open tabs stay attached); new requests get fresh
/// ids from `next_id`.
pub fn rebuild(
    rows: &[TreeRow],
    previous: &[CollectionItem],
    next_id: &mut usize,
) -> Vec<CollectionItem> {
    let mut old_ids = HashMap::new();
    collect_ids(previous, &mut old_ids);

    let present: HashSet<&str> = rows
        .iter()
        .filter(|r| r.kind == ItemKind::Folder)
        .map(|r| r.uid.as_str())
        .collect();
    let mut by_parent: HashMap<Option<&str>, Vec<&TreeRow>> = HashMap::new();
    for row in rows {
        let parent = row
            .parent_uid
            .as_deref()
            .filter(|p| present.contains(p) && *p != row.uid);
        by_parent.entry(parent).or_default().push(row);
    }
    for siblings in by_parent.values_mut() {
        siblings.sort_by(|a, b| (&a.sort_key, &a.uid).cmp(&(&b.sort_key, &b.uid)));
    }

    let mut visited = HashSet::new();
    build_level(None, &by_parent, &old_ids, next_id, &mut visited)
}

fn build_level(
    parent: Option<&str>,
    by_parent: &HashMap<Option<&str>, Vec<&TreeRow>>,
    old_ids: &HashMap<String, usize>,
    next_id: &mut usize,
    visited: &mut HashSet<String>,
) -> Vec<CollectionItem> {
    let Some(rows) = by_parent.get(&parent) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(rows.len());
    // app addresses folders by name, so siblings must not share one
    // (two clients can each create "New Folder" offline)
    let mut folder_names = HashSet::new();
    for row in rows {
        // corrupt parent chain can't loop us forever
        if !visited.insert(row.uid.clone()) {
            continue;
        }
        match row.kind {
            ItemKind::Request => {
                let Ok(mut node) = serde_json::from_value::<PostmanRequestNode>(row.data.clone())
                else {
                    continue;
                };
                node.uid = Some(row.uid.clone());
                node.id = old_ids.get(&row.uid).copied().unwrap_or_else(|| {
                    let id = *next_id;
                    *next_id += 1;
                    id
                });
                out.push(CollectionItem::Request(node));
            }
            ItemKind::Folder => {
                let data: FolderData =
                    serde_json::from_value(row.data.clone()).unwrap_or(FolderData {
                        name: "Untitled Folder".to_string(),
                        description: None,
                        event: None,
                        protocol_profile_behavior: None,
                    });
                let name = (1..)
                    .map(|n| match n {
                        1 => data.name.clone(),
                        n => format!("{} ({n})", data.name),
                    })
                    .find(|candidate| !folder_names.contains(candidate))
                    .unwrap();
                folder_names.insert(name.clone());
                let children = build_level(Some(&row.uid), by_parent, old_ids, next_id, visited);
                out.push(CollectionItem::Folder(PostmanFolder {
                    name,
                    uid: Some(row.uid.clone()),
                    protocol_profile_behavior: data.protocol_profile_behavior,
                    item: children,
                    event: data.event,
                    description: data.description,
                    unsaved: false,
                }));
            }
        }
    }
    out
}

fn collect_ids(items: &[CollectionItem], out: &mut HashMap<String, usize>) {
    for item in items {
        match item {
            CollectionItem::Request(r) => {
                if let Some(uid) = &r.uid {
                    out.insert(uid.clone(), r.id);
                }
            }
            CollectionItem::Folder(f) => collect_ids(&f.item, out),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rustrest_core::collection::model::{CollectionInfo, PostmanRequestDetails, PostmanUrl};

    pub(crate) fn request(id: usize, name: &str) -> CollectionItem {
        CollectionItem::Request(PostmanRequestNode {
            id,
            uid: None,
            name: name.to_string(),
            event: None,
            request: PostmanRequestDetails {
                method: "GET".to_string(),
                url: Some(PostmanUrl::String(format!("https://api.test/{name}"))),
                header: None,
                body: None,
                auth: None,
                description: None,
            },
            unsaved: false,
            response: None,
            protocol_request: None,
        })
    }

    pub(crate) fn folder(name: &str, item: Vec<CollectionItem>) -> CollectionItem {
        CollectionItem::Folder(PostmanFolder {
            name: name.to_string(),
            uid: None,
            protocol_profile_behavior: None,
            item,
            event: None,
            description: Some(format!("about {name}")),
            unsaved: false,
        })
    }

    pub(crate) fn collection(item: Vec<CollectionItem>) -> PostmanCollection {
        PostmanCollection {
            id: 1,
            file_path: None,
            storage_dir: None,
            remote_dir: None,
            unsaved: false,
            info: CollectionInfo {
                name: "Demo".to_string(),
                postman_id: None,
                schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                    .to_string(),
                description: None,
            },
            item,
            variable: None,
            auth: None,
            event: None,
        }
    }

    fn rows_of(col: &PostmanCollection) -> Vec<TreeRow> {
        let (flat, children) = flatten(col);
        let mut keys = HashMap::new();
        for uids in children.values() {
            for (uid, key) in uids.iter().zip(crate::sort_key::evenly_spaced(uids.len())) {
                keys.insert(uid.clone(), key);
            }
        }
        flat.into_iter()
            .map(|f| TreeRow {
                sort_key: keys[&f.uid].clone(),
                uid: f.uid,
                parent_uid: f.parent_uid,
                kind: f.kind,
                data: f.data,
            })
            .collect()
    }

    #[test]
    fn ensure_uids_fills_gaps_and_fixes_duplicates() {
        let mut col = collection(vec![request(1, "a"), folder("f", vec![request(2, "b")])]);
        assert!(ensure_uids(&mut col));
        assert!(!ensure_uids(&mut col), "second pass must be a no-op");

        // a duplicated uid (e.g. a copied file) is replaced
        let CollectionItem::Request(a) = &col.item[0] else {
            unreachable!()
        };
        let dup = a.uid.clone();
        let CollectionItem::Folder(f) = &mut col.item[1] else {
            unreachable!()
        };
        let CollectionItem::Request(b) = &mut f.item[0] else {
            unreachable!()
        };
        b.uid = dup.clone();
        assert!(ensure_uids(&mut col));
        let (flat, _) = flatten(&col);
        let uids: HashSet<_> = flat.iter().map(|f| f.uid.clone()).collect();
        assert_eq!(uids.len(), 3);
    }

    #[test]
    fn flatten_then_rebuild_round_trips() {
        let mut col = collection(vec![
            request(1, "a"),
            folder(
                "f",
                vec![request(2, "b"), folder("g", vec![request(3, "c")])],
            ),
            request(4, "d"),
        ]);
        ensure_uids(&mut col);
        let mut next_id = 100;
        let rebuilt = rebuild(&rows_of(&col), &col.item, &mut next_id);

        assert_eq!(
            serde_json::to_value(&rebuilt).unwrap(),
            serde_json::to_value(&col.item).unwrap()
        );
        assert_eq!(next_id, 100, "known requests keep their runtime ids");
    }

    #[test]
    fn rebuild_keeps_orphans_and_dedupes_folder_names() {
        let mut col = collection(vec![
            folder("New Folder", vec![]),
            folder("New Folder", vec![request(1, "x")]),
        ]);
        ensure_uids(&mut col);
        let mut rows = rows_of(&col);
        rows.push(TreeRow {
            uid: new_uid(),
            parent_uid: Some(new_uid()), // parent doesn't exist
            kind: ItemKind::Request,
            sort_key: "z".to_string(),
            data: canonical_data(ItemKind::Request, &rows[2].data),
        });

        let mut next_id = 10;
        let rebuilt = rebuild(&rows, &col.item, &mut next_id);
        let names: Vec<String> = rebuilt
            .iter()
            .map(|i| match i {
                CollectionItem::Folder(f) => f.name.clone(),
                CollectionItem::Request(r) => r.name.clone(),
            })
            .collect();
        assert_eq!(names, vec!["New Folder", "New Folder (2)", "x"]);
        assert_eq!(next_id, 11, "the orphan is a new request");
    }

    #[test]
    fn canonical_data_is_stable() {
        let mut col = collection(vec![request(1, "a")]);
        ensure_uids(&mut col);
        let (flat, _) = flatten(&col);
        let once = canonical_data(ItemKind::Request, &flat[0].data);
        assert_eq!(once, flat[0].data);
        assert_eq!(canonical_data(ItemKind::Request, &once), once);
        assert!(once.get("id").is_none());
    }
}
