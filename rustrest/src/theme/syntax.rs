//! Syntax highlighting driven by a theme's Zed-style `syntax` table.
//!
//! iced's own highlighter only knows syntect's handful of bundled themes,
//! so this is a copy of it that takes an arbitrary syntect `Theme` instead

use super::schema::{StyleMap, parse_color};
use iced::advanced::text::highlighter::{self, Format};
use iced::font::{self, Font};
use iced::{Color, Theme};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};
use two_face::re_exports::syntect;

use syntect::highlighting::{
    self, FontStyle, ScopeSelectors, StyleModifier, ThemeItem, ThemeSettings,
};
use syntect::parsing;

static SYNTAXES: LazyLock<parsing::SyntaxSet> = LazyLock::new(two_face::syntax::extra_no_newlines);

static BUNDLED: LazyLock<highlighting::ThemeSet> =
    LazyLock::new(highlighting::ThemeSet::load_defaults);

/// syntect themes built from theme files, keyed by a hash of their source.
/// They're leaked so highlighters can borrow them for `'static` (as iced's does with its bundled set),
/// the cache keeps that bounded to one copy per distinct syntax table ever loaded.
static BUILT: LazyLock<Mutex<HashMap<u64, &'static highlighting::Theme>>> =
    LazyLock::new(Default::default);

/// a syntect theme usable by the editor's highlighter. Compared by identity, which is exactly "did the theme change".
#[derive(Debug, Clone, Copy)]
pub struct SyntaxTheme(&'static highlighting::Theme);

impl PartialEq for SyntaxTheme {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl SyntaxTheme {
    /// the look the script editor had before themes: Solarized on dark themes, InspiredGitHub on light ones.
    pub fn fallback(dark: bool) -> Self {
        let key = if dark {
            "Solarized (dark)"
        } else {
            "InspiredGitHub"
        };
        SyntaxTheme(&BUNDLED.themes[key])
    }

    /// builds one from a Zed `style` map's `syntax` table, or `None` if it doesn't have one.
    pub fn from_style(style: &StyleMap, foreground: Color, background: Color) -> Option<Self> {
        let syntax = style.get("syntax")?.as_object()?;
        if syntax.is_empty() {
            return None;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(syntax)
            .unwrap_or_default()
            .hash(&mut hasher);
        format!("{foreground:?}{background:?}").hash(&mut hasher);
        let key = hasher.finish();

        let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(theme) = built.get(&key) {
            return Some(SyntaxTheme(theme));
        }

        let mut scopes = Vec::new();
        // later entries win in syntect, so emit general captures before specific ones ("string" before "string.escape").
        let mut entries: Vec<_> = syntax.iter().collect();
        entries.sort_by_key(|(name, _)| name.matches('.').count());
        for (name, value) in entries {
            let Some(style) = token_style(value) else {
                continue;
            };
            for selector in scopes_for(name) {
                if let Ok(scope) = ScopeSelectors::from_str(selector) {
                    scopes.push(ThemeItem { scope, style });
                }
            }
        }

        let theme: &'static highlighting::Theme = Box::leak(Box::new(highlighting::Theme {
            name: None,
            author: None,
            settings: ThemeSettings {
                foreground: Some(to_syntect(foreground)),
                background: Some(to_syntect(background)),
                ..Default::default()
            },
            scopes,
        }));
        built.insert(key, theme);
        Some(SyntaxTheme(theme))
    }
}

fn to_syntect(c: Color) -> highlighting::Color {
    let [r, g, b, a] = c.into_rgba8();
    highlighting::Color { r, g, b, a }
}

/// a Zed syntax entry: `{ "color": "#..", "font_style": "italic", "font_weight": 700 }`.
fn token_style(value: &serde_json::Value) -> Option<StyleModifier> {
    let foreground = value
        .get("color")
        .and_then(|c| c.as_str())
        .and_then(parse_color)
        .map(to_syntect);

    let mut font_style = FontStyle::empty();
    if value.get("font_style").and_then(|s| s.as_str()) == Some("italic") {
        font_style |= FontStyle::ITALIC;
    }
    if value
        .get("font_weight")
        .and_then(|w| w.as_f64())
        .is_some_and(|w| w >= 600.0)
    {
        font_style |= FontStyle::BOLD;
    }

    if foreground.is_none() && font_style.is_empty() {
        return None;
    }
    Some(StyleModifier {
        foreground,
        background: None,
        font_style: (!font_style.is_empty()).then_some(font_style),
    })
}

/// TextMate scopes that correspond to a Zed syntax capture name.
fn scopes_for(capture: &str) -> &'static [&'static str] {
    match capture {
        "comment" => &["comment", "punctuation.definition.comment"],
        "comment.doc" => &["comment.block.documentation", "comment.line.documentation"],
        "string" => &["string", "punctuation.definition.string"],
        "string.escape" => &["constant.character.escape"],
        "string.regex" => &["string.regexp"],
        "string.special" | "string.special.symbol" => &["constant.other.symbol"],
        "number" => &["constant.numeric"],
        "boolean" => &[
            "constant.language.boolean",
            "constant.language.true",
            "constant.language.false",
        ],
        "constant" => &["constant", "constant.language", "support.constant"],
        "keyword" => &[
            "keyword",
            "storage.type",
            "storage.modifier",
            "keyword.control",
            "variable.language.this",
        ],
        "operator" => &["keyword.operator"],
        "function" => &[
            "entity.name.function",
            "support.function",
            "meta.function-call",
        ],
        "function.method" => &["meta.function-call.method", "entity.name.function.method"],
        "constructor" => &[
            "entity.name.type.class",
            "meta.class entity.name",
            "new.expr entity.name",
        ],
        "type" => &[
            "entity.name.type",
            "support.type",
            "support.class",
            "entity.name.class",
        ],
        "type.builtin" => &["support.type.builtin", "support.type.primitive"],
        "variable" => &["variable", "variable.other"],
        "variable.special" => &["variable.language", "support.variable"],
        "variable.parameter" => &["variable.parameter"],
        "property" => &[
            "variable.other.property",
            "variable.other.object.property",
            "support.variable.property",
            "meta.object-literal.key",
            "support.type.property-name",
        ],
        "attribute" => &["entity.other.attribute-name"],
        "tag" => &["entity.name.tag"],
        "punctuation" => &["punctuation"],
        "punctuation.bracket" => &[
            "punctuation.section",
            "meta.brace",
            "punctuation.definition.block",
        ],
        "punctuation.delimiter" => &["punctuation.separator", "punctuation.terminator"],
        "punctuation.special" => &["punctuation.definition.template-expression"],
        "embedded" => &["meta.embedded", "source.embedded"],
        "label" => &["entity.name.label"],
        "link_uri" => &["markup.underline.link"],
        "link_text" => &["string.other.link"],
        "title" => &["markup.heading", "entity.name.section"],
        "emphasis" => &["markup.italic"],
        "emphasis.strong" => &["markup.bold"],
        "enum" => &["entity.name.type.enum"],
        "namespace" => &["entity.name.namespace", "entity.name.module"],
        _ => &[],
    }
}

/// `text_editor::highlight_with` settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: SyntaxTheme,
    /// file extension or language name, as for iced's highlighter.
    pub token: String,
}

const LINES_PER_SNAPSHOT: usize = 50;

pub struct Highlighter {
    syntax: &'static parsing::SyntaxReference,
    highlighter: highlighting::Highlighter<'static>,
    caches: Vec<(parsing::ParseState, parsing::ScopeStack)>,
    current_line: usize,
}

fn find_syntax(token: &str) -> &'static parsing::SyntaxReference {
    SYNTAXES
        .find_syntax_by_token(token)
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text())
}

impl highlighter::Highlighter for Highlighter {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = Box<dyn Iterator<Item = (Range<usize>, Self::Highlight)> + 'a>;

    fn new(settings: &Self::Settings) -> Self {
        let syntax = find_syntax(&settings.token);
        Highlighter {
            syntax,
            highlighter: highlighting::Highlighter::new(settings.theme.0),
            caches: vec![(parsing::ParseState::new(syntax), parsing::ScopeStack::new())],
            current_line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.syntax = find_syntax(&new_settings.token);
        self.highlighter = highlighting::Highlighter::new(new_settings.theme.0);
        self.change_line(0);
    }

    fn change_line(&mut self, line: usize) {
        let snapshot = line / LINES_PER_SNAPSHOT;
        if snapshot <= self.caches.len() {
            self.caches.truncate(snapshot);
            self.current_line = snapshot * LINES_PER_SNAPSHOT;
        } else {
            self.caches.truncate(1);
            self.current_line = 0;
        }
        let (parser, stack) = self.caches.last().cloned().unwrap_or_else(|| {
            (
                parsing::ParseState::new(self.syntax),
                parsing::ScopeStack::new(),
            )
        });
        self.caches.push((parser, stack));
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        if self.current_line / LINES_PER_SNAPSHOT >= self.caches.len() {
            let (parser, stack) = self.caches.last().expect("caches are never empty");
            self.caches.push((parser.clone(), stack.clone()));
        }
        self.current_line += 1;

        let (parser, stack) = self.caches.last_mut().expect("caches are never empty");
        let ops = parser.parse_line(line, &SYNTAXES).unwrap_or_default();
        let highlighter = &self.highlighter;

        Box::new(
            ScopeRangeIterator {
                ops,
                line_length: line.len(),
                index: 0,
                last_str_index: 0,
            }
            .filter_map(move |(range, op)| {
                let _ = stack.apply(&op);
                (!range.is_empty()).then(|| {
                    (
                        range,
                        Highlight(highlighter.style_mod_for_stack(&stack.scopes)),
                    )
                })
            }),
        )
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

#[derive(Debug)]
pub struct Highlight(StyleModifier);

impl Highlight {
    pub fn to_format(&self, _theme: &Theme) -> Format<Font> {
        let color = self
            .0
            .foreground
            .map(|c| Color::from_rgba8(c.r, c.g, c.b, c.a as f32 / 255.0));
        let font = self.0.font_style.and_then(|style| {
            let bold = style.contains(FontStyle::BOLD);
            let italic = style.contains(FontStyle::ITALIC);
            (bold || italic).then_some(Font {
                weight: if bold {
                    font::Weight::Bold
                } else {
                    font::Weight::Normal
                },
                style: if italic {
                    font::Style::Italic
                } else {
                    font::Style::Normal
                },
                ..Font::MONOSPACE
            })
        });
        Format { color, font }
    }
}

struct ScopeRangeIterator {
    ops: Vec<(usize, parsing::ScopeStackOp)>,
    line_length: usize,
    index: usize,
    last_str_index: usize,
}

impl Iterator for ScopeRangeIterator {
    type Item = (Range<usize>, parsing::ScopeStackOp);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index > self.ops.len() {
            return None;
        }
        let next_str_i = if self.index == self.ops.len() {
            self.line_length
        } else {
            self.ops[self.index].0
        };
        let range = self.last_str_index..next_str_i;
        self.last_str_index = next_str_i;
        let op = if self.index == 0 {
            parsing::ScopeStackOp::Noop
        } else {
            self.ops[self.index - 1].1.clone()
        };
        self.index += 1;
        Some((range, op))
    }
}

/// a fast, line-local JSON highlighter for the response body and the raw
/// JSON request body.
///
/// syntect re-parses with regexes, and iced re-highlights every line from
/// the first changed one down, which made big bodies crawl (folding a node
/// near the top re-highlighted the whole document). pretty-printed JSON
/// never carries state across lines, so a hand tokenizer is enough.
/// the colors still come from the theme via the same TextMate scopes.
pub struct JsonHighlighter {
    styles: JsonStyles,
    current_line: usize,
}

#[derive(Clone, Copy)]
struct JsonStyles {
    key: StyleModifier,
    string: StyleModifier,
    number: StyleModifier,
    boolean: StyleModifier,
    null: StyleModifier,
    bracket: StyleModifier,
    delimiter: StyleModifier,
    variable: StyleModifier,
}

impl JsonStyles {
    fn new(theme: SyntaxTheme) -> Self {
        let highlighter = highlighting::Highlighter::new(theme.0);
        let style = |scopes: &str| {
            let stack = parsing::ScopeStack::from_str(scopes).unwrap_or_default();
            highlighter.style_mod_for_stack(stack.as_slice())
        };
        JsonStyles {
            key: style(
                "source.json meta.mapping.key.json string.quoted.double.json support.type.property-name.json",
            ),
            string: style("source.json meta.mapping.value.json string.quoted.double.json"),
            number: style("source.json constant.numeric.json"),
            boolean: style("source.json constant.language.boolean.json"),
            null: style("source.json constant.language.null.json"),
            bracket: style("source.json punctuation.section.mapping.begin.json"),
            delimiter: style("source.json punctuation.separator.mapping.pair.json"),
            variable: style("source.json variable.other.json"),
        }
    }
}

impl highlighter::Highlighter for JsonHighlighter {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        JsonHighlighter {
            styles: JsonStyles::new(settings.theme),
            current_line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.styles = JsonStyles::new(new_settings.theme);
        self.current_line = 0;
    }

    fn change_line(&mut self, line: usize) {
        // lines are independent, so there's nothing to rewind but the counter
        self.current_line = self.current_line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.current_line += 1;
        json_spans(line, &self.styles)
            .into_iter()
            .map(|(range, style)| (range, Highlight(style)))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

fn json_spans(line: &str, styles: &JsonStyles) -> Vec<(Range<usize>, StyleModifier)> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let style = match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                let rest = line[i..].trim_start();
                if rest.starts_with(':') {
                    styles.key
                } else {
                    styles.string
                }
            }
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                // `{{variable}}` placeholder in a request body
                i = line[i..].find("}}").map_or(bytes.len(), |end| i + end + 2);
                styles.variable
            }
            b'{' | b'}' | b'[' | b']' => {
                i += 1;
                styles.bracket
            }
            b',' | b':' => {
                i += 1;
                styles.delimiter
            }
            b'-' | b'0'..=b'9' => {
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                styles.number
            }
            c if c.is_ascii_alphabetic() => {
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                match &line[start..i] {
                    "true" | "false" => styles.boolean,
                    "null" => styles.null,
                    _ => continue,
                }
            }
            _ => {
                // whitespace, the fold placeholder and anything else stays plain;
                // step a whole char so ranges stay on UTF-8 boundaries
                i += line[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
        };
        spans.push((start..i, style));
    }
    spans
}
