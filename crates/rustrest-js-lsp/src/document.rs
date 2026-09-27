//! Open documents and their script kind.

use crate::{
    analysis::{self, Local, Masks},
    diagnostics::{self, RawDiagnostic},
    position::LineIndex,
};

/// Which runtime API a script sees, derived from its URI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    PreRequest,
    PostResponse,
    /// exposes the union of both APIs without availability warnings
    Unknown,
}

impl DocKind {
    /// `rustrest:///scripts/<id>/pre-request.js` -> `PreRequest`,
    /// `.../post-response.js` -> `PostResponse`, anything else -> `Unknown`.
    pub fn from_uri(uri: &str) -> Self {
        let path = uri.split(['?', '#']).next().unwrap_or(uri);
        if path.ends_with("pre-request.js") {
            DocKind::PreRequest
        } else if path.ends_with("post-response.js") {
            DocKind::PostResponse
        } else {
            DocKind::Unknown
        }
    }
}

/// An open text document with its cached analysis.
pub struct Document {
    pub text: String,
    pub version: i32,
    pub kind: DocKind,
    pub index: LineIndex,
    pub masks: Masks,
    pub locals: Vec<Local>,
    pub diagnostics: Vec<RawDiagnostic>,
}

impl Document {
    pub fn new(text: String, version: i32, kind: DocKind) -> Self {
        let index = LineIndex::new(&text);
        let masks = Masks::new(&text);
        let (diagnostics, locals) = diagnostics::analyze(&text, kind);
        let locals = locals.unwrap_or_else(|| analysis::scan_locals(&text, &masks));
        Self {
            text,
            version,
            kind,
            index,
            masks,
            locals,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_uri() {
        assert_eq!(
            DocKind::from_uri("rustrest:///scripts/abc/pre-request.js"),
            DocKind::PreRequest
        );
        assert_eq!(
            DocKind::from_uri("rustrest:///scripts/abc/post-response.js"),
            DocKind::PostResponse
        );
        assert_eq!(DocKind::from_uri("file:///tmp/other.js"), DocKind::Unknown);
    }

    #[test]
    fn never_panics_on_partial_input() {
        use crate::{completion, hover};
        let inputs = [
            "pm.test('a', function () { pm.expect(pm.response.json().x).to.be.",
            "`${pm.response.code} ${ {a: [1, (2}` + 'é😀\\",
            ")]}.(.[.\"'`/*//",
            "let { a, [b]: c = (1, 2 } = ((x) => x.",
            "pm.sendRequest({ url: 'http://x', header: [{ key: 'a', value: 'b' }] }, (err, res) => { res.",
            "...pm..environment.?.get(",
            "é",
            "",
        ];
        for src in inputs {
            for kind in [DocKind::PreRequest, DocKind::PostResponse, DocKind::Unknown] {
                let doc = Document::new(src.to_string(), 0, kind);
                for offset in 0..=src.len() + 2 {
                    let _ = completion::complete(&doc.text, &doc.masks, offset, kind, &doc.locals);
                    let _ = hover::hover(&doc.text, &doc.masks, offset, &doc.locals);
                    let _ = hover::signature_help(&doc.text, &doc.masks, offset, &doc.locals);
                }
            }
        }
    }
}
