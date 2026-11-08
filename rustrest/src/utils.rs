use crate::collection::collection::{CollectionItem, PostmanCollection};
use crate::collection_adapter::{RequestNodeTabExt, sync_body_from_tab};
use crate::message::SidebarDragItem;
use crate::ui::tab::Tab;

pub use rustrest_core::collection::tree_ops::{
    clone_items_with_new_ids, contains_request_node_by_id, copy_name, duplicate_folder,
    duplicate_request, find_request_mut, insert_item_at, insert_nested, insert_nested_named,
    insert_nested_request, remove_nested, remove_nested_request, rename_nested_folder, take_folder,
    take_request,
};

/// relocates a dragged request/folder from its source location to `dest_folder_path`
/// within `dest_collection_id` (that folder's children, or the collection root when
/// empty). when `before_request_id` is set, the moved item is inserted immediately
/// before that sibling request; otherwise it's appended at the end.
pub fn move_sidebar_item(
    collections: &mut Vec<PostmanCollection>,
    source: SidebarDragItem,
    dest_collection_id: usize,
    dest_folder_path: Vec<String>,
    before_request_id: Option<usize>,
) {
    // guard against dropping a folder into itself or one of its own descendants
    if let SidebarDragItem::Folder {
        collection_id,
        path,
    } = &source
    {
        if *collection_id == dest_collection_id && dest_folder_path.starts_with(path.as_slice()) {
            return;
        }
    }

    let extracted = match &source {
        SidebarDragItem::Request {
            collection_id,
            parent_path,
            request_id,
        } => collections
            .iter_mut()
            .find(|c| c.id == *collection_id)
            .and_then(|c| take_request(&mut c.item, parent_path, *request_id)),
        SidebarDragItem::Folder {
            collection_id,
            path,
        } => collections
            .iter_mut()
            .find(|c| c.id == *collection_id)
            .and_then(|c| take_folder(&mut c.item, path)),
    };

    let Some(mut item) = extracted else {
        return;
    };

    // moving is a structural change, so flag it for saving same as add/rename do
    match &mut item {
        CollectionItem::Request(req) => req.unsaved = true,
        CollectionItem::Folder(folder) => folder.unsaved = true,
    }

    if let Some(dest_col) = collections.iter_mut().find(|c| c.id == dest_collection_id) {
        insert_item_at(
            &mut dest_col.item,
            &dest_folder_path,
            item,
            before_request_id,
        );
    }
}

pub fn format_json_or_fallback(raw_body: &str) -> String {
    let is_valid = serde_json::from_str::<serde::de::IgnoredAny>(raw_body).is_ok();
    match is_valid.then(|| beautify_json(raw_body)).flatten() {
        Some(pretty) => pretty,
        None => format!("// Invalid JSON:\n{}", raw_body),
    }
}

/// pretty-prints JSON with 2-space indentation, Postman "Beautify" style.
pub fn beautify_json(raw: &str) -> Option<String> {
    const INDENT: &str = "  ";

    fn newline(out: &mut String, depth: usize) {
        out.push('\n');
        for _ in 0..depth {
            out.push_str(INDENT);
        }
    }

    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len() * 2);
    let mut stack: Vec<char> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                // copy the whole string literal verbatim, honouring escapes
                out.push(c);
                i += 1;
                loop {
                    let ch = *chars.get(i)?;
                    out.push(ch);
                    i += 1;
                    match ch {
                        '\\' => {
                            out.push(*chars.get(i)?);
                            i += 1;
                        }
                        '"' => break,
                        _ => {}
                    }
                }
                continue;
            }
            '{' if chars.get(i + 1) == Some(&'{') => {
                // `{{variable}}` placeholder outside a string
                let start = i;
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '}' && chars[i + 1] == '}') {
                    i += 1;
                }
                if i + 1 >= chars.len() {
                    return None;
                }
                i += 2;
                out.extend(&chars[start..i]);
                continue;
            }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                let next = chars[i + 1..].iter().position(|ch| !ch.is_whitespace());
                if let Some(offset) = next.filter(|&o| chars[i + 1 + o] == close) {
                    // keep empty containers on one line: `{}` / `[]`
                    out.push(c);
                    out.push(close);
                    i += offset + 2;
                    continue;
                }
                out.push(c);
                stack.push(close);
                newline(&mut out, stack.len());
            }
            '}' | ']' => {
                if stack.pop()? != c {
                    return None;
                }
                newline(&mut out, stack.len());
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, stack.len());
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            _ => out.push(c),
        }
        i += 1;
    }

    (stack.is_empty() && !out.is_empty()).then_some(out)
}

// recursively updates node in the collection by its ID, syncing tab state with the request
pub fn update_node(items: &mut Vec<CollectionItem>, target_id: usize, tab: &Tab) -> bool {
    let Some(req) = rustrest_core::collection::tree_ops::find_request_mut(items, target_id) else {
        return false;
    };

    // sync name / method / url / headers / pre-request and test scripts
    req.update_from_tab(tab);

    // sync request body conditionally on the active body type
    sync_body_from_tab(req, tab);

    true
}

/// opens `dir` in the OS file manager (Explorer, Finder, or the xdg default).
pub fn open_in_file_manager(dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    if let Err(e) = std::process::Command::new(program).arg(dir).spawn() {
        eprintln!("Failed to open {}: {e}", dir.display());
    }
}
