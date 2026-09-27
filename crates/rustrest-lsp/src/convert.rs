//! LSP JSON -> client types.

use crate::position::{Encoding, range_from_lsp};
use crate::{CompletionItem, CompletionKind, Diagnostic, Severity};
use serde_json::Value;

/// `textDocument/publishDiagnostics` `diagnostics` array. `text` is the
/// document's latest contents (for column conversion).
pub fn diagnostics(value: &Value, text: &str, enc: Encoding) -> Vec<Diagnostic> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|d| {
            let severity = match d.get("severity").and_then(Value::as_u64) {
                Some(2) => Severity::Warning,
                Some(3) => Severity::Info,
                Some(4) => Severity::Hint,
                _ => Severity::Error,
            };
            let message = d.get("message").and_then(Value::as_str).unwrap_or_default();
            let message = match d.get("source").and_then(Value::as_str) {
                Some(source) if !source.is_empty() => format!("{message} ({source})"),
                _ => message.to_string(),
            };
            Diagnostic {
                range: range_from_lsp(text, &d["range"], enc),
                severity,
                message,
            }
        })
        .collect()
}

/// LSP `CompletionItemKind` number -> client kind.
pub fn completion_kind(kind: Option<u64>) -> CompletionKind {
    match kind {
        Some(2) => CompletionKind::Method,
        Some(3 | 4) => CompletionKind::Function,
        Some(5 | 10) => CompletionKind::Property,
        Some(6) => CompletionKind::Variable,
        Some(9) => CompletionKind::Module,
        Some(14) => CompletionKind::Keyword,
        Some(20 | 21) => CompletionKind::Constant,
        _ => CompletionKind::Other,
    }
}

/// a `textDocument/completion` result: `CompletionItem[]`,
/// `CompletionList`, or null.
pub fn completions(result: &Value) -> Vec<CompletionItem> {
    let items = match result {
        Value::Array(items) => items,
        Value::Object(list) => match list.get("items").and_then(Value::as_array) {
            Some(items) => items,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items.iter().filter_map(completion).collect()
}

fn completion(item: &Value) -> Option<CompletionItem> {
    let label = item.get("label")?.as_str()?.to_string();
    let snippet = item.get("insertTextFormat").and_then(Value::as_u64) == Some(2);
    let insert_text = item
        .pointer("/textEdit/newText")
        .or_else(|| item.get("insertText"))
        .and_then(Value::as_str)
        .map(|t| {
            if snippet {
                strip_snippet(t)
            } else {
                t.to_string()
            }
        });
    Some(CompletionItem {
        kind: completion_kind(item.get("kind").and_then(Value::as_u64)),
        detail: non_empty(
            item.get("detail")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        documentation: non_empty(item.get("documentation").map(markup_text)),
        insert_text,
        label,
    })
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

/// `$1`, `${2:default}`, `$0` -> plain text (placeholders keep their
/// default text).
pub fn strip_snippet(snippet: &str) -> String {
    let mut out = String::with_capacity(snippet.len());
    let mut chars = snippet.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '$' if chars.peek().is_some_and(|c| c.is_ascii_digit()) => {
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    chars.next();
                }
            }
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    chars.next();
                }
                if chars.peek() == Some(&':') {
                    chars.next();
                }
                // copy the default text up to the matching brace
                let mut depth = 1;
                for c in chars.by_ref() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// plain text of a `string`, `MarkupContent` or `MarkedString`.
fn markup_text(value: &Value) -> String {
    match value {
        Value::String(s) => strip_fences(s),
        Value::Object(o) => o
            .get("value")
            .and_then(Value::as_str)
            .map(strip_fences)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn strip_fences(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// a `textDocument/hover` result -> plain text, `None` if empty.
pub fn hover(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    let parts: Vec<String> = match contents {
        Value::Array(items) => items.iter().map(markup_text).collect(),
        other => vec![markup_text(other)],
    };
    let text = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;
    use serde_json::json;

    #[test]
    fn diagnostics_mapping() {
        let text = "let é = ;";
        let v = json!([
            {"range": {"start": {"line": 0, "character": 8}, "end": {"line": 0, "character": 9}},
             "severity": 2, "message": "hm", "source": "js"},
            {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}},
             "message": "bad"}
        ]);
        let d = diagnostics(&v, text, Encoding::Utf16);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].message, "hm (js)");
        assert_eq!(d[0].range.start, Position { line: 0, column: 9 });
        assert_eq!(
            d[0].range.end,
            Position {
                line: 0,
                column: 10
            }
        );
        assert_eq!(d[1].severity, Severity::Error);
        let d = diagnostics(&v, text, Encoding::Utf8);
        assert_eq!(d[0].range.start.column, 8);
        assert!(diagnostics(&Value::Null, text, Encoding::Utf8).is_empty());
    }

    #[test]
    fn completion_shapes() {
        let item = json!({"label": "get", "kind": 2, "detail": "(key: string) => any",
            "documentation": {"kind": "markdown", "value": "```ts\nx\n```\nGets a var"}});
        let list = completions(&json!({"isIncomplete": false, "items": [item.clone()]}));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "get");
        assert_eq!(list[0].kind, CompletionKind::Method);
        assert_eq!(list[0].detail.as_deref(), Some("(key: string) => any"));
        assert_eq!(list[0].documentation.as_deref(), Some("x\nGets a var"));
        assert_eq!(list[0].insert_text, None);

        let arr = completions(&json!([
            {"label": "a", "kind": 6, "insertText": "aa", "documentation": "doc"},
            {"label": "b", "kind": 3, "insertText": "zz", "textEdit": {"newText": "b()", "range": {}}},
            {"label": "c", "insertText": "c(${1:x}, $2)$0", "insertTextFormat": 2},
            {"nolabel": true}
        ]));
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].insert_text.as_deref(), Some("aa"));
        assert_eq!(arr[0].documentation.as_deref(), Some("doc"));
        assert_eq!(arr[1].insert_text.as_deref(), Some("b()"));
        assert_eq!(arr[1].kind, CompletionKind::Function);
        assert_eq!(arr[2].insert_text.as_deref(), Some("c(x, )"));
        assert_eq!(arr[2].kind, CompletionKind::Other);
        assert!(completions(&Value::Null).is_empty());
    }

    #[test]
    fn kinds() {
        assert_eq!(completion_kind(Some(10)), CompletionKind::Property);
        assert_eq!(completion_kind(Some(5)), CompletionKind::Property);
        assert_eq!(completion_kind(Some(9)), CompletionKind::Module);
        assert_eq!(completion_kind(Some(14)), CompletionKind::Keyword);
        assert_eq!(completion_kind(Some(21)), CompletionKind::Constant);
        assert_eq!(completion_kind(Some(7)), CompletionKind::Other);
        assert_eq!(completion_kind(None), CompletionKind::Other);
    }

    #[test]
    fn snippets() {
        assert_eq!(
            strip_snippet("for (${1:i} of ${2:list}) {\n\t$0\n}"),
            "for (i of list) {\n\t\n}"
        );
        assert_eq!(strip_snippet("a\\$1${1}"), "a$1");
        assert_eq!(strip_snippet("${1:{nested}}"), "{nested}");
    }

    #[test]
    fn hover_shapes() {
        assert_eq!(
            hover(&json!({"contents": {"kind": "markdown", "value": "```js\nfn()\n```\ndocs"}})),
            Some("fn()\ndocs".to_string())
        );
        assert_eq!(
            hover(&json!({"contents": "plain"})),
            Some("plain".to_string())
        );
        assert_eq!(
            hover(&json!({"contents": [{"language": "js", "value": "x: number"}, "more", ""]})),
            Some("x: number\n\nmore".to_string())
        );
        assert_eq!(hover(&json!({"contents": []})), None);
        assert_eq!(hover(&Value::Null), None);
    }
}
