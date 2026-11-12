//! Changes the model asks for in its replies: a fenced ```request-patch block
//! that edits the active request, or a ```collection-action block that
//! creates/renames/moves/deletes collection-tree items.

use rustrest_plugin_api::{CollectionOperation, RequestPatch, RightPanelAction, UiNode};

/// What a finished reply asks the app to do. The system prompt tells the
/// model never to send both kinds, so at most one applies.
#[derive(Debug, Clone)]
pub(crate) enum ReplyAction {
    PatchRequest(Box<RequestPatch>),
    ChangeCollections(Vec<CollectionOperation>),
}

impl ReplyAction {
    /// The action in a reply's text, if any. Collection operations win over a
    /// request patch if a reply somehow contains both.
    pub(crate) fn from_reply(text: &str) -> Option<Self> {
        let ops = extract_collection_ops(text);
        if !ops.is_empty() {
            return Some(ReplyAction::ChangeCollections(ops));
        }
        extract_patch(text).map(|patch| ReplyAction::PatchRequest(Box::new(patch)))
    }

    /// The panel response that renders `tree` and hands this action to the
    /// host (which asks the user to confirm destructive ones).
    pub(crate) fn into_panel_action(self, tree: UiNode) -> RightPanelAction {
        match self {
            ReplyAction::PatchRequest(patch) => {
                RightPanelAction::UpdateUiAndApplyPatch(tree, *patch)
            }
            // the single-op variant keeps working against older hosts
            ReplyAction::ChangeCollections(mut ops) if ops.len() == 1 => {
                RightPanelAction::UpdateUiAndProposeCollectionOp(tree, ops.remove(0))
            }
            ReplyAction::ChangeCollections(ops) => {
                RightPanelAction::UpdateUiAndProposeCollectionOps(tree, ops)
            }
        }
    }
}

/// Contents of every fenced code block in `text` whose info string is
/// exactly `lang` (so e.g. ```json doesn't also match ```jsonc).
fn fenced_blocks<'a>(text: &'a str, lang: &str) -> Vec<&'a str> {
    let fence = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&fence) {
        let after = &rest[start + fence.len()..];
        let Some(line_end) = after.find('\n') else {
            break;
        };
        // the info string must end right after `lang`
        if !after[..line_end].trim().is_empty() {
            rest = after;
            continue;
        }
        let body = &after[line_end + 1..];
        let Some(end) = body.find("```") else {
            break;
        };
        blocks.push(body[..end].trim());
        rest = &body[end + 3..];
    }
    blocks
}

/// The last ```request-patch block in `text` that decodes to a patch actually
/// changing something (every field is optional, so any JSON object would otherwise "decode").
fn extract_patch(text: &str) -> Option<RequestPatch> {
    fenced_blocks(text, "request-patch")
        .into_iter()
        .rev()
        .filter_map(|block| serde_json::from_str::<RequestPatch>(block).ok())
        .find(|patch| !patch.is_empty())
}

/// The operations in the last ```collection-action block in `text` that
/// decodes, either as a single operation object or an array of them.
fn extract_collection_ops(text: &str) -> Vec<CollectionOperation> {
    fenced_blocks(text, "collection-action")
        .into_iter()
        .rev()
        .find_map(|block| {
            serde_json::from_str::<Vec<CollectionOperation>>(block)
                .ok()
                .or_else(|| {
                    serde_json::from_str::<CollectionOperation>(block)
                        .ok()
                        .map(|op| vec![op])
                })
        })
        .unwrap_or_default()
}
