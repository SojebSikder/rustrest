//! Cheap, error-tolerant textual analysis used while the user is typing:
//! string/comment masks, member-chain extraction, enclosing-call lookup and a
//! fallback scan for locally declared names.

use crate::api_model::{self, GLOBAL, Member, TypeDef};

/// Byte class of a source position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Code,
    Str,
    Comment,
}

/// A string/template/comment span; `open` spans (line comments, unterminated
/// literals) also contain the position right at their end.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: usize,
    end: usize,
    class: Class,
    open: bool,
}

/// Per-byte classification of a text into code, string and comment.
#[derive(Debug, Clone)]
pub struct Masks {
    classes: Vec<Class>,
    spans: Vec<Span>,
}

impl Masks {
    pub fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let n = bytes.len();
        let mut spans = Vec::new();
        // brace depth of each open `${ ... }` template substitution
        let mut templates: Vec<u32> = Vec::new();
        let mut i = 0;
        while i < n {
            match bytes[i] {
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    let end = memchr(bytes, i, b'\n').unwrap_or(n);
                    spans.push(Span {
                        start: i,
                        end,
                        class: Class::Comment,
                        open: true,
                    });
                    i = end;
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    let (end, closed) = match find(bytes, i + 2, b"*/") {
                        Some(p) => (p + 2, true),
                        None => (n, false),
                    };
                    spans.push(Span {
                        start: i,
                        end,
                        class: Class::Comment,
                        open: !closed,
                    });
                    i = end;
                }
                q @ (b'"' | b'\'') => {
                    let mut j = i + 1;
                    let mut closed = false;
                    while j < n {
                        match bytes[j] {
                            b'\\' => j += 2,
                            b'\n' => break,
                            c if c == q => {
                                j += 1;
                                closed = true;
                                break;
                            }
                            _ => j += 1,
                        }
                    }
                    let end = j.min(n);
                    spans.push(Span {
                        start: i,
                        end,
                        class: Class::Str,
                        open: !closed,
                    });
                    i = end;
                }
                b'`' => {
                    i = scan_template(bytes, i, &mut spans, &mut templates);
                }
                b'{' if !templates.is_empty() => {
                    *templates.last_mut().unwrap() += 1;
                    i += 1;
                }
                b'}' if !templates.is_empty() => {
                    let depth = templates.last_mut().unwrap();
                    if *depth == 0 {
                        templates.pop();
                        i = scan_template(bytes, i, &mut spans, &mut templates);
                    } else {
                        *depth -= 1;
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }
        let mut classes = vec![Class::Code; n];
        for s in &spans {
            classes[s.start..s.end.min(n)].fill(s.class);
        }
        Self { classes, spans }
    }

    pub fn class(&self, i: usize) -> Class {
        self.classes.get(i).copied().unwrap_or(Class::Code)
    }

    /// Class of the caret position `offset` (between bytes `offset-1` and `offset`).
    pub fn class_at(&self, offset: usize) -> Class {
        let idx = self.spans.partition_point(|s| s.start < offset);
        match idx.checked_sub(1).map(|k| self.spans[k]) {
            Some(s) if offset < s.end || (offset == s.end && s.open) => s.class,
            _ => Class::Code,
        }
    }

    fn span_start(&self, i: usize) -> usize {
        let idx = self.spans.partition_point(|s| s.start <= i);
        idx.checked_sub(1).map(|k| self.spans[k].start).unwrap_or(i)
    }
}

/// Scans template text starting at the opening backtick or the `}` closing
/// a substitution; returns the index to continue from.
fn scan_template(
    bytes: &[u8],
    start: usize,
    spans: &mut Vec<Span>,
    templates: &mut Vec<u32>,
) -> usize {
    let n = bytes.len();
    let mut j = start + 1;
    while j < n {
        match bytes[j] {
            b'\\' => j += 2,
            b'`' => {
                spans.push(Span {
                    start,
                    end: j + 1,
                    class: Class::Str,
                    open: false,
                });
                return j + 1;
            }
            b'$' if bytes.get(j + 1) == Some(&b'{') => {
                spans.push(Span {
                    start,
                    end: j + 2,
                    class: Class::Str,
                    open: false,
                });
                templates.push(0);
                return j + 2;
            }
            _ => j += 1,
        }
    }
    spans.push(Span {
        start,
        end: n,
        class: Class::Str,
        open: true,
    });
    n
}

fn memchr(bytes: &[u8], from: usize, b: u8) -> Option<usize> {
    bytes[from..].iter().position(|&c| c == b).map(|p| p + from)
}

fn find(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from > bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + from)
}

/// Identifier byte (non-ASCII bytes count so unicode names stay whole).
pub fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// One step of a member-expression chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    Name(String),
    /// `(...)` call
    Call,
    /// `[...]` computed access
    Index,
    /// any non-identifier root (literal, parenthesized expression, ...)
    Other,
}

/// Member chain ending at the cursor: `pm.expect(x).to.eq|` ->
/// segs `[pm, expect, Call, to]`, prefix `eq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain {
    pub segs: Vec<Seg>,
    pub prefix: String,
    pub prefix_start: usize,
}

fn skip_ws_back(bytes: &[u8], masks: &Masks, mut j: usize) -> usize {
    while j > 0 && (bytes[j - 1].is_ascii_whitespace() || masks.class(j - 1) == Class::Comment) {
        j -= 1;
    }
    j
}

/// Index of the opener matching the closer at `close`, ignoring literals.
fn match_open(bytes: &[u8], masks: &Masks, close: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        if masks.class(i) != Class::Code {
            continue;
        }
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extracts the member chain ending at `offset`. Empty `segs` means the cursor
/// is on a bare identifier (top-level completion).
pub fn extract_chain(text: &str, masks: &Masks, offset: usize) -> Chain {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let mut i = offset;
    while i > 0 && is_ident_byte(bytes[i - 1]) && masks.class(i - 1) == Class::Code {
        i -= 1;
    }
    let prefix = String::from_utf8_lossy(&bytes[i..offset]).into_owned();
    let mut chain = Chain {
        segs: Vec::new(),
        prefix,
        prefix_start: i,
    };

    let mut j = skip_ws_back(bytes, masks, i);
    if !is_member_dot(bytes, masks, j) {
        return chain;
    }
    let mut rev = Vec::new();
    loop {
        // consume `.` / `?.`
        j -= 1;
        if j > 0 && bytes[j - 1] == b'?' {
            j -= 1;
        }
        j = skip_ws_back(bytes, masks, j);
        // one primary expression with call/index postfixes, read backwards
        loop {
            if j == 0 {
                push_other(&mut rev);
                break;
            }
            let c = bytes[j - 1];
            if masks.class(j - 1) == Class::Str {
                rev.push(Seg::Other);
                j = masks.span_start(j - 1);
                break;
            }
            if c == b')' || c == b']' {
                let Some(open) = match_open(bytes, masks, j - 1) else {
                    rev.push(Seg::Other);
                    j = 0;
                    break;
                };
                rev.push(if c == b')' { Seg::Call } else { Seg::Index });
                j = skip_ws_back(bytes, masks, open);
                continue;
            }
            if is_ident_byte(c) {
                let end = j;
                while j > 0 && is_ident_byte(bytes[j - 1]) && masks.class(j - 1) == Class::Code {
                    j -= 1;
                }
                let name = String::from_utf8_lossy(&bytes[j..end]).into_owned();
                if name.as_bytes()[0].is_ascii_digit() {
                    rev.push(Seg::Other);
                } else {
                    rev.push(Seg::Name(name));
                }
                break;
            }
            push_other(&mut rev);
            break;
        }
        if rev.last() == Some(&Seg::Other) {
            break;
        }
        let k = skip_ws_back(bytes, masks, j);
        if is_member_dot(bytes, masks, k) {
            j = k;
        } else {
            break;
        }
    }
    rev.reverse();
    chain.segs = rev;
    chain
}

/// A trailing call/index group with no callee is a parenthesized expression
/// or array literal: replace it with `Other`.
fn push_other(rev: &mut Vec<Seg>) {
    if matches!(rev.last(), Some(Seg::Call | Seg::Index)) {
        rev.pop();
    }
    rev.push(Seg::Other);
}

/// `bytes[j-1]` is a member-access `.` (not part of `...` or a number).
fn is_member_dot(bytes: &[u8], masks: &Masks, j: usize) -> bool {
    j > 0
        && bytes[j - 1] == b'.'
        && masks.class(j - 1) == Class::Code
        && !(j > 1 && bytes[j - 2] == b'.')
}

/// What a chain prefix resolves to.
#[derive(Clone, Copy, Debug)]
pub enum Resolved {
    Type(&'static TypeDef),
    Func(&'static Member),
    Unknown,
}

fn member_resolved(m: &'static Member) -> Resolved {
    if m.is_function() {
        Resolved::Func(m)
    } else {
        api_model::type_def(m.ty).map_or(Resolved::Unknown, Resolved::Type)
    }
}

/// Resolves chain segments through the type graph. Roots shadowed by a local
/// declaration resolve to `Unknown`.
pub fn resolve(segs: &[Seg], locals: &[Local]) -> Resolved {
    let mut cur = Resolved::Type(&GLOBAL);
    for (i, seg) in segs.iter().enumerate() {
        cur = match (seg, cur) {
            (Seg::Name(n), _) if i == 0 && locals.iter().any(|l| l.name == *n) => Resolved::Unknown,
            (Seg::Name(n), Resolved::Type(t)) => {
                t.member(n).map_or(Resolved::Unknown, member_resolved)
            }
            (Seg::Call, Resolved::Func(m)) => {
                api_model::type_def(m.ty).map_or(Resolved::Unknown, Resolved::Type)
            }
            _ => Resolved::Unknown,
        };
        if matches!(cur, Resolved::Unknown) {
            break;
        }
    }
    cur
}

/// Display qualifier of a chain: `pm.environment` when made of plain names,
/// else the resolved owner type name (e.g. `Assertion`).
pub fn qualifier(segs: &[Seg], owner: &TypeDef) -> String {
    let names: Option<Vec<&str>> = segs
        .iter()
        .map(|s| match s {
            Seg::Name(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    match names {
        Some(names) => names.join("."),
        None => owner.name.to_string(),
    }
}

/// A known function whose argument list encloses the cursor.
#[derive(Debug)]
pub struct CallInfo {
    pub member: &'static Member,
    pub qualifier: String,
    pub active_param: u32,
}

/// Finds the innermost call of a known API function whose argument list
/// contains `offset`.
pub fn enclosing_call(
    text: &str,
    masks: &Masks,
    offset: usize,
    locals: &[Local],
) -> Option<CallInfo> {
    let bytes = text.as_bytes();
    let mut i = offset.min(bytes.len());
    let mut depth = 0usize;
    let mut commas = 0u32;
    while i > 0 {
        i -= 1;
        if masks.class(i) != Class::Code {
            continue;
        }
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
            b'(' => {
                let callee_end = skip_ws_back(bytes, masks, i);
                let chain = extract_chain(text, masks, callee_end);
                if let Some(info) = lookup_call(&chain, locals, commas) {
                    return Some(info);
                }
                commas = 0;
            }
            b'[' | b'{' => commas = 0,
            b',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    None
}

fn lookup_call(chain: &Chain, locals: &[Local], commas: u32) -> Option<CallInfo> {
    if chain.prefix.is_empty() {
        return None;
    }
    let Resolved::Type(owner) = resolve(&chain.segs, locals) else {
        return None;
    };
    if chain.segs.is_empty() && locals.iter().any(|l| l.name == chain.prefix) {
        return None;
    }
    let member = owner.member(&chain.prefix)?;
    if !member.is_function() {
        return None;
    }
    let n = member.params.map_or(0, |p| p.len()) as u32;
    let variadic = member
        .params
        .and_then(|p| p.last())
        .is_some_and(|p| p.name.starts_with("..."));
    let active_param = if variadic {
        commas.min(n.saturating_sub(1))
    } else {
        commas
    };
    Some(CallInfo {
        member,
        qualifier: qualifier(&chain.segs, owner),
        active_param,
    })
}

/// Kind of a locally declared name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalKind {
    Variable,
    Function,
    Class,
}

/// A locally declared name (variable, function, class or parameter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    pub kind: LocalKind,
}

#[derive(Debug, PartialEq, Eq)]
enum Tok<'a> {
    Ident(&'a str),
    Punct(u8),
    Arrow,
    Literal,
}

fn tokenize<'a>(text: &'a str, masks: &Masks) -> Vec<Tok<'a>> {
    let bytes = text.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match masks.class(i) {
            Class::Comment => i += 1,
            Class::Str => {
                if toks.last() != Some(&Tok::Literal) {
                    toks.push(Tok::Literal);
                }
                i += 1;
            }
            Class::Code => {
                let b = bytes[i];
                if is_ident_byte(b) {
                    let start = i;
                    while i < bytes.len()
                        && is_ident_byte(bytes[i])
                        && masks.class(i) == Class::Code
                    {
                        i += 1;
                    }
                    match text.get(start..i) {
                        Some(s) if !s.as_bytes()[0].is_ascii_digit() => toks.push(Tok::Ident(s)),
                        _ => toks.push(Tok::Literal),
                    }
                } else if b == b'=' && bytes.get(i + 1) == Some(&b'>') {
                    toks.push(Tok::Arrow);
                    i += 2;
                } else {
                    if !b.is_ascii_whitespace() {
                        toks.push(Tok::Punct(b));
                    }
                    i += 1;
                }
            }
        }
    }
    toks
}

const DECL_STOP: &[&str] = &[
    "let", "const", "var", "function", "class", "if", "for", "while", "return",
];

/// Textual fallback for local declarations when the file doesn't parse:
/// `let|const|var|function|class <name>`, destructuring, function/arrow/catch params.
pub fn scan_locals(text: &str, masks: &Masks) -> Vec<Local> {
    let toks = tokenize(text, masks);
    let mut out: Vec<Local> = Vec::new();
    let mut add = |name: &str, kind: LocalKind| {
        if !out.iter().any(|l| l.name == name) && !api_model::KEYWORDS.contains(&name) {
            out.push(Local {
                name: name.to_string(),
                kind,
            });
        }
    };
    for (i, tok) in toks.iter().enumerate() {
        match tok {
            Tok::Ident("let" | "const" | "var") => {
                let mut k = i + 1;
                loop {
                    k = collect_pattern(&toks, k, &mut |n| add(n, LocalKind::Variable));
                    // skip the initializer up to a top-level `,` (next declarator) or `;`
                    let mut depth = 0i32;
                    let mut next = None;
                    while k < toks.len() {
                        match &toks[k] {
                            Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
                            Tok::Punct(b')' | b']' | b'}') => {
                                depth -= 1;
                                if depth < 0 {
                                    break;
                                }
                            }
                            Tok::Punct(b';') if depth == 0 => break,
                            Tok::Punct(b',') if depth == 0 => {
                                next = Some(k + 1);
                                break;
                            }
                            Tok::Ident(w) if depth == 0 && DECL_STOP.contains(w) => break,
                            _ => {}
                        }
                        k += 1;
                    }
                    match next {
                        Some(n) => k = n,
                        None => break,
                    }
                }
            }
            Tok::Ident("function") => {
                let mut k = i + 1;
                if let Some(Tok::Ident(name)) = toks.get(k) {
                    add(name, LocalKind::Function);
                    k += 1;
                }
                if toks.get(k) == Some(&Tok::Punct(b'(')) {
                    collect_params(&toks, k, &mut |n| add(n, LocalKind::Variable));
                }
            }
            Tok::Ident("class") => {
                if let Some(Tok::Ident(name)) = toks.get(i + 1) {
                    add(name, LocalKind::Class);
                }
            }
            Tok::Ident("catch") => {
                if toks.get(i + 1) == Some(&Tok::Punct(b'(')) {
                    collect_params(&toks, i + 1, &mut |n| add(n, LocalKind::Variable));
                }
            }
            Tok::Arrow if i > 0 => match &toks[i - 1] {
                Tok::Ident(name) => add(name, LocalKind::Variable),
                Tok::Punct(b')') => {
                    let mut depth = 0i32;
                    let mut k = i - 1;
                    loop {
                        match toks[k] {
                            Tok::Punct(b')' | b']' | b'}') => depth += 1,
                            Tok::Punct(b'(' | b'[' | b'{') => depth -= 1,
                            _ => {}
                        }
                        if depth == 0 || k == 0 {
                            break;
                        }
                        k -= 1;
                    }
                    if toks[k] == Tok::Punct(b'(') {
                        collect_params(&toks, k, &mut |n| add(n, LocalKind::Variable));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

/// Collects a binding pattern starting at `k` (identifier or `{...}`/`[...]`);
/// returns the index after it.
fn collect_pattern(toks: &[Tok], k: usize, add: &mut dyn FnMut(&str)) -> usize {
    match toks.get(k) {
        Some(Tok::Ident(name)) => {
            add(name);
            k + 1
        }
        Some(Tok::Punct(b'{' | b'[')) => {
            let mut depth = 0i32;
            let mut j = k;
            while j < toks.len() {
                match &toks[j] {
                    Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
                    Tok::Punct(b')' | b']' | b'}') => {
                        depth -= 1;
                        if depth == 0 {
                            return j + 1;
                        }
                    }
                    Tok::Ident(name) if is_binding_name(toks, j) => add(name),
                    _ => {}
                }
                j += 1;
            }
            j
        }
        _ => k,
    }
}

/// Parameters of the `(` at `open`, up to its matching `)`.
fn collect_params(toks: &[Tok], open: usize, add: &mut dyn FnMut(&str)) {
    let mut depth = 0i32;
    for j in open..toks.len() {
        match &toks[j] {
            Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
            Tok::Punct(b')' | b']' | b'}') => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            Tok::Ident(name) if is_binding_name(toks, j) => add(name),
            _ => {}
        }
    }
}

/// In a pattern/param list, a name is bound when followed by a separator and
/// not preceded by `=` (default value) or `.`.
fn is_binding_name(toks: &[Tok], j: usize) -> bool {
    let next_ok = matches!(
        toks.get(j + 1),
        None | Some(Tok::Punct(b',' | b')' | b']' | b'}' | b'='))
    );
    let prev_ok = match j.checked_sub(1).map(|p| &toks[p]) {
        Some(Tok::Punct(b'=')) => false,
        Some(Tok::Punct(b'.')) => is_spread(toks, j),
        _ => true,
    };
    next_ok && prev_ok
}

fn is_spread(toks: &[Tok], j: usize) -> bool {
    j >= 3 && toks[j - 3..j].iter().all(|t| *t == Tok::Punct(b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(src: &str) -> Chain {
        let masks = Masks::new(src);
        extract_chain(src, &masks, src.len())
    }

    fn names(segs: &[Seg]) -> Vec<String> {
        segs.iter()
            .map(|s| match s {
                Seg::Name(n) => n.clone(),
                Seg::Call => "()".into(),
                Seg::Index => "[]".into(),
                Seg::Other => "?".into(),
            })
            .collect()
    }

    #[test]
    fn chain_simple() {
        let c = chain("pm.response.to.have.");
        assert_eq!(names(&c.segs), ["pm", "response", "to", "have"]);
        assert_eq!(c.prefix, "");
        let c = chain("console.l");
        assert_eq!(names(&c.segs), ["console"]);
        assert_eq!(c.prefix, "l");
        assert_eq!(c.prefix_start, 8);
        let c = chain("let x = foo");
        assert!(c.segs.is_empty());
        assert_eq!(c.prefix, "foo");
    }

    #[test]
    fn chain_nested_calls_and_strings() {
        let c = chain("pm.expect(foo.bar).to.eq");
        assert_eq!(names(&c.segs), ["pm", "expect", "()", "to"]);
        assert_eq!(c.prefix, "eq");
        let c = chain("pm.expect(JSON.parse(\"a)(\" + ')')[0]).to.be.");
        assert_eq!(names(&c.segs), ["pm", "expect", "()", "to", "be"]);
        let c = chain("pm.test('x', function () {\n  pm.expect(a)\n    .to.");
        assert_eq!(names(&c.segs), ["pm", "expect", "()", "to"]);
        let c = chain("pm.globals /* c */ . se");
        assert_eq!(names(&c.segs), ["pm", "globals"]);
        assert_eq!(c.prefix, "se");
        let c = chain("a?.b.");
        assert_eq!(names(&c.segs), ["a", "b"]);
        let c = chain("x[1].");
        assert_eq!(names(&c.segs), ["x", "[]"]);
    }

    #[test]
    fn chain_non_identifier_roots() {
        assert_eq!(names(&chain("\"abc\".").segs), ["?"]);
        assert_eq!(names(&chain("(a + b).").segs), ["?"]);
        assert_eq!(names(&chain("1.").segs), ["?"]);
        assert!(chain("[...").segs.is_empty());
    }

    #[test]
    fn masks_strings_comments_templates() {
        let src = "a = 'x' // c\n`t ${pm.} u` /* b */";
        let m = Masks::new(src);
        assert_eq!(m.class_at(5), Class::Str); // inside 'x'
        assert_eq!(m.class_at(7), Class::Code); // after closing quote
        assert_eq!(m.class_at(12), Class::Comment); // end of line comment
        let inner = src.find("pm.").unwrap() + 3;
        assert_eq!(m.class_at(inner), Class::Code);
        assert_eq!(m.class_at(inner + 3), Class::Str);
        assert_eq!(m.class_at(src.len() - 2), Class::Comment);
        // unterminated string runs to the end of its line
        let m = Masks::new("pm.set('ab");
        assert_eq!(m.class_at(10), Class::Str);
    }

    #[test]
    fn enclosing_call_finds_known_function() {
        let src = "pm.environment.set(\"k\", foo(1, 2";
        let m = Masks::new(src);
        let info = enclosing_call(src, &m, src.len(), &[]).unwrap();
        assert_eq!(info.member.name, "set");
        assert_eq!(info.qualifier, "pm.environment");
        assert_eq!(info.active_param, 1);

        let src = "pm.sendRequest({ url: 'x', method: ";
        let m = Masks::new(src);
        let info = enclosing_call(src, &m, src.len(), &[]).unwrap();
        assert_eq!(info.member.name, "sendRequest");
        assert_eq!(info.active_param, 0);

        let src = "pm.expect(1).to.equal(";
        let m = Masks::new(src);
        let info = enclosing_call(src, &m, src.len(), &[]).unwrap();
        assert_eq!(info.qualifier, "Assertion");

        let src = "foo(1, ";
        let m = Masks::new(src);
        assert!(enclosing_call(src, &m, src.len(), &[]).is_none());
    }

    #[test]
    fn scan_locals_fallback() {
        let src = "const a = 1, b = f(2);\nlet { c, d: e, g = h } = x;\nfunction fn1(p1, p2 = 3, ...rest) {\nconst arrow = (q, r) => q;\nlist.map(z => z.\ntry {} catch (err) {}\nclass K {}\nvar [m, n] = y;\npm.";
        let m = Masks::new(src);
        let locals = scan_locals(src, &m);
        let got: Vec<&str> = locals.iter().map(|l| l.name.as_str()).collect();
        for name in [
            "a", "b", "c", "e", "g", "fn1", "p1", "p2", "rest", "arrow", "q", "r", "z", "err", "K",
            "m", "n",
        ] {
            assert!(got.contains(&name), "missing {name} in {got:?}");
        }
        for name in ["d", "h", "f", "x", "y", "list", "pm"] {
            assert!(!got.contains(&name), "unexpected {name} in {got:?}");
        }
        assert_eq!(
            locals.iter().find(|l| l.name == "fn1").unwrap().kind,
            LocalKind::Function
        );
    }
}
