//! Completion items for the position before the cursor.

use lsp_types::{CompletionItem, CompletionItemKind, Documentation};

use crate::{
    analysis::{self, Class, Local, LocalKind, Masks, Resolved},
    api_model::{GLOBAL, KEYWORDS, Member, TypeDef},
    document::DocKind,
};

/// Completions at byte `offset`; empty inside strings/comments and after
/// unknown roots. Items are not filtered by the typed prefix.
pub fn complete(
    text: &str,
    masks: &Masks,
    offset: usize,
    kind: DocKind,
    locals: &[Local],
) -> Vec<CompletionItem> {
    if masks.class_at(offset) != Class::Code {
        return Vec::new();
    }
    let chain = analysis::extract_chain(text, masks, offset);
    if chain.segs.is_empty() {
        return top_level(kind, locals);
    }
    match analysis::resolve(&chain.segs, locals) {
        Resolved::Type(t) => members(t, kind),
        _ => Vec::new(),
    }
}

fn members(t: &TypeDef, kind: DocKind) -> Vec<CompletionItem> {
    t.members
        .iter()
        .filter(|m| m.available_in(kind))
        .map(|m| member_item(m, t))
        .collect()
}

fn member_item(m: &Member, owner: &TypeDef) -> CompletionItem {
    let global = std::ptr::eq(owner, &GLOBAL);
    let kind = match (m.is_function(), global) {
        (true, true) => CompletionItemKind::FUNCTION,
        (true, false) => CompletionItemKind::METHOD,
        (false, true) if m.ty.ends_with("Constructor") => CompletionItemKind::CLASS,
        (false, true) => CompletionItemKind::VARIABLE,
        (false, false) => CompletionItemKind::PROPERTY,
    };
    CompletionItem {
        label: m.name.to_string(),
        kind: Some(kind),
        detail: Some(m.detail()),
        documentation: Some(Documentation::String(m.full_doc())),
        ..Default::default()
    }
}

fn top_level(kind: DocKind, locals: &[Local]) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = locals
        .iter()
        .map(|l| CompletionItem {
            label: l.name.clone(),
            kind: Some(match l.kind {
                LocalKind::Variable => CompletionItemKind::VARIABLE,
                LocalKind::Function => CompletionItemKind::FUNCTION,
                LocalKind::Class => CompletionItemKind::CLASS,
            }),
            detail: Some("local".to_string()),
            ..Default::default()
        })
        .collect();
    items.extend(
        GLOBAL
            .members
            .iter()
            .filter(|m| m.available_in(kind) && !locals.iter().any(|l| l.name == m.name))
            .map(|m| member_item(m, &GLOBAL)),
    );
    items.extend(
        KEYWORDS
            .iter()
            .filter(|k| !locals.iter().any(|l| l.name == **k))
            .map(|k| CompletionItem {
                label: k.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("keyword".to_string()),
                ..Default::default()
            }),
    );
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn labels(src: &str, kind: DocKind) -> Vec<String> {
        let doc = Document::new(src.to_string(), 1, kind);
        complete(&doc.text, &doc.masks, src.len(), kind, &doc.locals)
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn pm_members_by_kind() {
        let post = labels("pm.", DocKind::PostResponse);
        for l in ["test", "expect", "response", "environment", "sendRequest"] {
            assert!(post.contains(&l.to_string()), "{l} missing");
        }
        assert!(!post.contains(&"setHeader".to_string()));

        let pre = labels("pm.", DocKind::PreRequest);
        assert!(pre.contains(&"setHeader".to_string()));
        assert!(!pre.contains(&"test".to_string()));
        assert!(!pre.contains(&"response".to_string()));

        let any = labels("pm.", DocKind::Unknown);
        assert!(any.contains(&"setHeader".to_string()) && any.contains(&"test".to_string()));
    }

    #[test]
    fn scope_assertion_response_console() {
        assert_eq!(
            labels("pm.environment.", DocKind::PreRequest),
            ["get", "set", "has", "unset", "clear"]
        );
        let a = labels(
            "pm.test('x', () => { pm.expect(1).to.",
            DocKind::PostResponse,
        );
        for l in ["equal", "eql", "be", "not", "true", "lengthOf"] {
            assert!(a.contains(&l.to_string()), "{l} missing");
        }
        let a = labels("pm.expect(x).to.be.eq", DocKind::PostResponse);
        assert!(a.contains(&"equal".to_string()));
        let r = labels("pm.response.to.have.", DocKind::PostResponse);
        for l in ["status", "header", "body", "ok", "not"] {
            assert!(r.contains(&l.to_string()), "{l} missing");
        }
        assert_eq!(
            labels("console.", DocKind::Unknown),
            ["log", "info", "warn", "error"]
        );
        assert!(labels("pm.response.json().", DocKind::PostResponse).is_empty());
    }

    #[test]
    fn top_level_and_locals() {
        let src = "const token = pm.environment.get('t');\nfunction helper(arg) {}\n";
        let l = labels(src, DocKind::PreRequest);
        for name in ["token", "helper", "arg", "pm", "console", "JSON", "const"] {
            assert!(l.contains(&name.to_string()), "{name} missing");
        }
        // mid-typing (doesn't parse): fallback scan still finds locals
        let l = labels(
            "let body = pm.response.json(\nconst x = bo",
            DocKind::PostResponse,
        );
        assert!(l.contains(&"body".to_string()));
        // local roots have no member completions
        assert!(labels("const res = 1;\nres.", DocKind::PostResponse).is_empty());
        // shadowed pm
        assert!(labels("const pm = {};\npm.", DocKind::PostResponse).is_empty());
    }

    #[test]
    fn nothing_in_strings_or_comments() {
        assert!(labels("pm.environment.get('pm.", DocKind::PreRequest).is_empty());
        assert!(labels("// pm.", DocKind::PreRequest).is_empty());
        assert!(labels("/* pm.", DocKind::PreRequest).is_empty());
    }
}
