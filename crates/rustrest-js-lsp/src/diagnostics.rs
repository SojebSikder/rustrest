//! Syntax/semantic errors from oxc plus API usage warnings.

use oxc_allocator::Allocator;
use oxc_ast::{
    AstKind,
    ast::{Expression, StaticMemberExpression},
};
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder, SymbolFlags};
use oxc_span::SourceType;

use crate::{
    analysis::{Local, LocalKind},
    api_model::{self, GLOBAL, Member, TypeDef},
    document::DocKind,
};

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A diagnostic over a byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDiagnostic {
    pub start: usize,
    pub end: usize,
    pub severity: Severity,
    pub message: String,
}

/// Parses `text` and returns its diagnostics, plus the declared locals when
/// the file parsed cleanly (`None` means use the textual fallback).
pub fn analyze(text: &str, kind: DocKind) -> (Vec<RawDiagnostic>, Option<Vec<Local>>) {
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, text, SourceType::script()).parse();
    let mut out: Vec<RawDiagnostic> = ret
        .diagnostics
        .iter()
        .map(|d| from_oxc(d, text.len()))
        .collect();
    if ret.fatal_error || !out.is_empty() {
        return (out, None);
    }

    let program = allocator.alloc(ret.program);
    let sem = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_build_nodes(true)
        .build(program);
    out.extend(sem.diagnostics.iter().map(|d| from_oxc(d, text.len())));
    let semantic = sem.semantic;

    let scoping = semantic.scoping();
    let mut locals: Vec<Local> = Vec::new();
    for id in scoping.symbol_ids() {
        let name = scoping.symbol_name(id);
        if locals.iter().any(|l| l.name == name) {
            continue;
        }
        let flags = scoping.symbol_flags(id);
        let kind = if flags.contains(SymbolFlags::Function) {
            LocalKind::Function
        } else if flags.contains(SymbolFlags::Class) {
            LocalKind::Class
        } else {
            LocalKind::Variable
        };
        locals.push(Local {
            name: name.to_string(),
            kind,
        });
    }

    for node in semantic.nodes().iter() {
        if let AstKind::StaticMemberExpression(m) = node.kind() {
            check_member(&semantic, m, kind, &mut out);
        }
    }
    (out, Some(locals))
}

fn from_oxc(d: &oxc_diagnostics::OxcDiagnostic, len: usize) -> RawDiagnostic {
    let span = d
        .labels
        .iter()
        .find(|l| l.primary())
        .or_else(|| d.labels.first())
        .map(|l| l.span());
    let (start, end) = span.map_or((0, 0), |s| (s.start as usize, s.end as usize));
    let mut message = d.message.to_string();
    if let Some(help) = &d.help {
        message.push('\n');
        message.push_str(help);
    }
    RawDiagnostic {
        start: start.min(len),
        end: end.clamp(start, len),
        severity: match d.severity {
            oxc_diagnostics::Severity::Error => Severity::Error,
            _ => Severity::Warning,
        },
        message,
    }
}

/// Static type of an expression in the API graph.
enum Ty {
    Type(&'static TypeDef),
    Func(&'static Member),
}

/// Resolves an expression rooted at an unshadowed global (`pm`, `console`, ...)
/// to its API type and display path.
fn resolve_expr(semantic: &Semantic, expr: &Expression) -> Option<(Ty, String)> {
    match expr {
        Expression::Identifier(id) => {
            let member = GLOBAL.member(id.name.as_str())?;
            if !semantic.is_reference_to_global_variable(id) {
                return None;
            }
            Some((member_ty(member)?, member.name.to_string()))
        }
        Expression::StaticMemberExpression(m) => {
            let (Ty::Type(t), path) = resolve_expr(semantic, &m.object)? else {
                return None;
            };
            let member = t.member(m.property.name.as_str())?;
            Some((member_ty(member)?, format!("{path}.{}", member.name)))
        }
        Expression::CallExpression(c) => {
            let (Ty::Func(f), path) = resolve_expr(semantic, &c.callee)? else {
                return None;
            };
            let t = api_model::type_def(f.ty)?;
            Some((Ty::Type(t), format!("{path}(...)")))
        }
        Expression::ParenthesizedExpression(p) => resolve_expr(semantic, &p.expression),
        _ => None,
    }
}

fn member_ty(m: &'static Member) -> Option<Ty> {
    if m.is_function() {
        Some(Ty::Func(m))
    } else {
        api_model::type_def(m.ty).map(Ty::Type)
    }
}

fn check_member(
    semantic: &Semantic,
    m: &StaticMemberExpression,
    kind: DocKind,
    out: &mut Vec<RawDiagnostic>,
) {
    let Some((Ty::Type(owner), path)) = resolve_expr(semantic, &m.object) else {
        return;
    };
    let name = m.property.name.as_str();
    let span = m.property.span;
    let message = match owner.member(name) {
        None if owner.closed => format!("Property '{name}' does not exist on '{path}'."),
        Some(member) if !member.available_in(kind) => {
            let only = match kind {
                DocKind::PreRequest => "post-response",
                _ => "pre-request",
            };
            format!("'{path}.{name}' is only available in {only} scripts.")
        }
        _ => return,
    };
    out.push(RawDiagnostic {
        start: span.start as usize,
        end: span.end as usize,
        severity: Severity::Warning,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags(src: &str, kind: DocKind) -> Vec<RawDiagnostic> {
        analyze(src, kind).0
    }

    #[test]
    fn syntax_error() {
        let d = diags("pm.test('bad', function () {", DocKind::PostResponse);
        assert!(!d.is_empty());
        assert!(d.iter().all(|d| d.severity == Severity::Error));
        let d = diags("let x = ;", DocKind::Unknown);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].start, 8);
        // semantic check: redeclaration
        let d = diags("let a = 1;\nlet a = 2;", DocKind::Unknown);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn unknown_member() {
        let src = "pm.enviroment.get(\"x\");";
        let d = diags(src, DocKind::PreRequest);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].message,
            "Property 'enviroment' does not exist on 'pm'."
        );
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(&src[d[0].start..d[0].end], "enviroment");

        let d = diags("pm.expect(1).to.be.equall(1);", DocKind::PostResponse);
        assert_eq!(
            d[0].message,
            "Property 'equall' does not exist on 'pm.expect(...).to.be'."
        );
        let d = diags(
            "console.debug('x'); pm.response.to.have.stat(1);",
            DocKind::PostResponse,
        );
        assert_eq!(d.len(), 2);

        // open types and shadowed roots don't warn
        let ok = "JSON.foo; pm.response.headers.etag; pm.response.json().anything;\n\
                  function f(pm) { pm.whatever(); }";
        assert!(diags(ok, DocKind::PostResponse).is_empty());
    }

    #[test]
    fn wrong_kind_member() {
        let d = diags("pm.test('t', () => {});", DocKind::PreRequest);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].message,
            "'pm.test' is only available in post-response scripts."
        );
        let d = diags("pm.setHeader('a', 'b');", DocKind::PostResponse);
        assert_eq!(
            d[0].message,
            "'pm.setHeader' is only available in pre-request scripts."
        );
        assert!(
            diags(
                "pm.setHeader('a', 'b'); pm.test('t', () => {});",
                DocKind::Unknown
            )
            .is_empty()
        );
    }

    #[test]
    fn locals_from_semantic() {
        let (_, locals) = analyze("const a = 1; function f(p) {} class C {}", DocKind::Unknown);
        let locals = locals.unwrap();
        let find = |n: &str| locals.iter().find(|l| l.name == n).map(|l| l.kind);
        assert_eq!(find("a"), Some(LocalKind::Variable));
        assert_eq!(find("f"), Some(LocalKind::Function));
        assert_eq!(find("p"), Some(LocalKind::Variable));
        assert_eq!(find("C"), Some(LocalKind::Class));
    }
}
