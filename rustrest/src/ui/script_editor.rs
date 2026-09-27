//! JavaScript editor

use crate::ui::context_menu::with_context_menu;
use crate::ui::modal::muted_text_color;
use iced::keyboard::{self, key::Named};
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Status};
use iced::widget::{
    Stack, button, column, container, pin, responsive, row, space, text, text_editor,
};
use iced::{
    Alignment, Border, Color, Element, Font, Length, Shadow, Size, Theme, Vector, highlighter,
};
use rustrest_core::script_engine::check_syntax;
use rustrest_lsp::{CompletionItem, CompletionKind, Diagnostic, Position, Severity};
use std::cell::Cell;
use std::sync::Arc;

const FONT_SIZE: f32 = 13.0;
const PADDING: f32 = 10.0;
// iced's default relative line height, and a typical monospace advance;
// used to place the completion popup next to the cursor
const LINE_HEIGHT: f32 = FONT_SIZE * 1.3;
const CHAR_WIDTH: f32 = FONT_SIZE * 0.6;
const POPUP_WIDTH: f32 = 360.0;
const POPUP_ROWS: usize = 8;
const POPUP_ROW_HEIGHT: f32 = 24.0;
const POPUP_DOC_HEIGHT: f32 = 48.0;
const MAX_DIAGNOSTICS_SHOWN: usize = 3;

#[derive(Debug, Clone)]
pub enum ScriptEditorEvent {
    Action(Action),
    TriggerCompletion,
    CompletionMove(isize),
    CompletionAccept,
    /// index into the currently shown (filtered) items
    CompletionPick(usize),
    CompletionDismiss,
}

impl ScriptEditorEvent {
    pub fn is_edit(&self) -> bool {
        match self {
            Self::Action(action) => action.is_edit(),
            Self::CompletionAccept | Self::CompletionPick(_) => true,
            _ => false,
        }
    }
}

/// language-server state for one editor, driven by `app::script_intel`.
#[derive(Debug, Clone, Default)]
pub struct LspState {
    /// `Some` once a language plugin has published for this editor; replaces
    /// the built-in syntax check.
    pub diagnostics: Option<Vec<Diagnostic>>,
    pub hover: Option<String>,
    pub completion: Option<Completion>,
    /// set by typing / Ctrl+Space, consumed when the request is sent
    pub want_completion: bool,
    /// set whenever the cursor moves
    pub want_hover: bool,
    /// (plugin generation, revision) last sent to the language plugin
    pub synced: Option<(u64, i32)>,
}

#[derive(Debug, Clone)]
pub struct Completion {
    /// start of the word being completed
    anchor: Position,
    items: Vec<CompletionItem>,
    /// indices into `items` matching the typed prefix, best first
    filtered: Vec<usize>,
    selected: usize,
}

#[derive(Debug, Clone)]
pub struct ScriptContent {
    content: text_editor::Content,
    syntax_error: Option<String>,
    revision: i32,
    /// estimate of the editor's first visible line, for placing the popup
    top_line: usize,
    visible_lines: Cell<usize>,
    pub lsp: LspState,
}

impl ScriptContent {
    pub fn with_text(text: &str) -> Self {
        Self {
            content: text_editor::Content::with_text(text),
            syntax_error: check_syntax(text).err(),
            revision: 0,
            top_line: 0,
            visible_lines: Cell::new(1),
            lsp: LspState::default(),
        }
    }

    /// applies an editor event; returns whether the text changed.
    pub fn update(&mut self, event: ScriptEditorEvent) -> bool {
        match event {
            ScriptEditorEvent::Action(action) => self.perform(action),
            ScriptEditorEvent::TriggerCompletion => {
                self.lsp.want_completion = true;
                false
            }
            ScriptEditorEvent::CompletionMove(delta) => {
                if let Some(completion) = &mut self.lsp.completion {
                    let len = completion.filtered.len() as isize;
                    if len > 0 {
                        completion.selected =
                            (completion.selected as isize + delta).rem_euclid(len) as usize;
                    }
                }
                false
            }
            ScriptEditorEvent::CompletionAccept => {
                let selected = self.lsp.completion.as_ref().map(|c| c.selected);
                selected.is_some_and(|index| self.accept(index))
            }
            ScriptEditorEvent::CompletionPick(index) => self.accept(index),
            ScriptEditorEvent::CompletionDismiss => {
                self.lsp.completion = None;
                false
            }
        }
    }

    fn perform(&mut self, action: Action) -> bool {
        let typed = match &action {
            Action::Edit(Edit::Insert(c)) => Some(*c),
            _ => None,
        };
        let scroll = match &action {
            Action::Scroll { lines } => Some(*lines),
            _ => None,
        };
        let is_edit = action.is_edit();

        self.content.perform(action);
        self.follow_cursor(scroll);
        if scroll.is_none() {
            self.lsp.want_hover = true;
        }
        if is_edit {
            self.edited();
        }

        match typed {
            Some('.') => {
                self.lsp.completion = None;
                self.lsp.want_completion = true;
            }
            Some(c) if is_ident_char(c) => {
                if self.lsp.completion.is_some() {
                    self.refilter();
                } else {
                    let prefix = self.word_prefix();
                    if prefix.chars().count() == 1 && !c.is_ascii_digit() {
                        self.lsp.want_completion = true;
                    }
                }
            }
            Some(_) => self.lsp.completion = None,
            None => self.refilter(),
        }
        is_edit
    }

    fn edited(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.syntax_error = check_syntax(&self.content.text()).err();
    }

    /// mirrors how the editor scrolls: just enough to keep the cursor visible.
    fn follow_cursor(&mut self, scroll: Option<i32>) {
        let last_line = self.content.line_count().saturating_sub(1) as i64;
        if let Some(lines) = scroll {
            self.top_line = (self.top_line as i64 + lines as i64).clamp(0, last_line) as usize;
            return;
        }
        let visible = self.visible_lines.get().max(1);
        let line = self.content.cursor().position.line;
        if line < self.top_line {
            self.top_line = line;
        } else if line >= self.top_line + visible {
            self.top_line = line + 1 - visible;
        }
    }

    fn accept(&mut self, index: usize) -> bool {
        let Some(completion) = self.lsp.completion.take() else {
            return false;
        };
        let Some(item) = completion
            .filtered
            .get(index)
            .and_then(|&i| completion.items.get(i))
        else {
            return false;
        };

        for _ in 0..self.word_prefix().chars().count() {
            self.content.perform(Action::Edit(Edit::Backspace));
        }
        let insert = item
            .insert_text
            .clone()
            .unwrap_or_else(|| item.label.clone());
        self.content
            .perform(Action::Edit(Edit::Paste(Arc::new(insert))));
        self.edited();
        self.follow_cursor(None);
        self.lsp.want_hover = true;
        true
    }

    /// re-applies the typed prefix to an open popup, closing it once the
    /// cursor leaves the word or nothing matches.
    fn refilter(&mut self) {
        let Some(anchor) = self.lsp.completion.as_ref().map(|c| c.anchor) else {
            return;
        };
        let cursor = self.cursor_position();
        let prefix = (cursor.line == anchor.line && cursor.column >= anchor.column)
            .then(|| {
                self.line_text(cursor.line as usize)
                    .get(anchor.column as usize..cursor.column as usize)
                    .map(str::to_owned)
            })
            .flatten()
            .filter(|p| p.chars().all(is_ident_char));

        let Some(prefix) = prefix else {
            self.lsp.completion = None;
            return;
        };
        if let Some(completion) = &mut self.lsp.completion {
            completion.filtered = filter_items(&completion.items, &prefix);
            completion.selected = 0;
            if completion.filtered.is_empty() {
                self.lsp.completion = None;
            }
        }
    }

    /// shows `items` for a completion requested at `anchor`, unless the
    /// cursor has since moved out of that word.
    pub fn open_completion(&mut self, anchor: Position, items: Vec<CompletionItem>) {
        if items.is_empty() {
            return;
        }
        self.lsp.completion = Some(Completion {
            anchor,
            filtered: Vec::new(),
            items,
            selected: 0,
        });
        self.refilter();
    }

    pub fn reset_lsp(&mut self) {
        self.lsp = LspState::default();
    }

    pub fn revision(&self) -> i32 {
        self.revision
    }

    pub fn cursor_position(&self) -> Position {
        let position = self.content.cursor().position;
        Position {
            line: position.line as u32,
            column: position.column as u32,
        }
    }

    /// start of the identifier the cursor is in (or right after).
    pub fn word_start(&self) -> Position {
        let cursor = self.cursor_position();
        let prefix_len = self.word_prefix().len() as u32;
        Position {
            line: cursor.line,
            column: cursor.column - prefix_len,
        }
    }

    fn word_prefix(&self) -> String {
        let cursor = self.cursor_position();
        let line = self.line_text(cursor.line as usize);
        let before = line.get(..cursor.column as usize).unwrap_or_default();
        let start = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_ident_char(*c))
            .last()
            .map_or(before.len(), |(i, _)| i);
        before[start..].to_string()
    }

    fn line_text(&self, line: usize) -> String {
        self.content
            .line(line)
            .map(|l| l.text.into_owned())
            .unwrap_or_default()
    }

    /// 1-based character column, for display.
    fn display_column(&self, position: Position) -> usize {
        let line = self.line_text(position.line as usize);
        line.get(..position.column as usize)
            .map_or(position.column as usize, |s| s.chars().count())
            + 1
    }

    fn shown_completion(&self) -> Option<&Completion> {
        self.lsp
            .completion
            .as_ref()
            .filter(|c| !c.filtered.is_empty())
    }

    pub fn text(&self) -> String {
        self.content.text()
    }

    pub fn selection(&self) -> Option<String> {
        self.content.selection()
    }

    /// Selected text, or the whole script when nothing is selected.
    pub fn selection_or_text(&self) -> String {
        self.selection().unwrap_or_else(|| self.text())
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// prefix matches first, then substring matches, keeping server order.
fn filter_items(items: &[CompletionItem], prefix: &str) -> Vec<usize> {
    let prefix = prefix.to_lowercase();
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let label = item.label.to_lowercase();
        if label.starts_with(&prefix) {
            starts.push(i);
        } else if label.contains(&prefix) {
            contains.push(i);
        }
    }
    starts.extend(contains);
    starts
}

/// Bordered, full-height script editor. `on_right_click` opens the context
/// menu. Syntax errors (or a language plugin's diagnostics) and hover info
/// are shown below the editor.
pub fn script_editor<'a, Message>(
    script: &'a ScriptContent,
    theme: &Theme,
    on_event: impl Fn(ScriptEditorEvent) -> Message + Copy + 'a,
    on_right_click: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let highlight_theme = if theme.extended_palette().is_dark {
        highlighter::Theme::SolarizedDark
    } else {
        highlighter::Theme::InspiredGitHub
    };

    let editor_area = responsive(move |size| {
        let visible = ((size.height - 2.0 * PADDING) / LINE_HEIGHT)
            .floor()
            .max(1.0);
        script.visible_lines.set(visible as usize);

        let popup_open = script.shown_completion().is_some();
        let editor = with_context_menu(
            text_editor(&script.content)
                .highlight("js", highlight_theme)
                .font(Font::MONOSPACE)
                .size(FONT_SIZE)
                .on_action(move |action| on_event(ScriptEditorEvent::Action(action)))
                .key_binding(move |key_press| key_binding(key_press, popup_open, on_event))
                .height(Length::Fill)
                .padding(PADDING),
            on_right_click.clone(),
        );

        // always a stack, so the editor keeps its state (focus) when the popup opens
        let mut layers = Stack::new().push(editor);
        if let Some(popup) = completion_popup(script, size, on_event) {
            layers = layers.push(popup);
        }
        layers.into()
    });

    let mut col = column![
        container(editor_area)
            .height(Length::Fill)
            .style(container::bordered_box)
    ]
    .spacing(4)
    .height(Length::Fill);

    match &script.lsp.diagnostics {
        Some(diagnostics) => {
            for diagnostic in diagnostics.iter().take(MAX_DIAGNOSTICS_SHOWN) {
                let start = diagnostic.range.start;
                let label = format!(
                    "Ln {}, Col {}: {}",
                    start.line + 1,
                    script.display_column(start),
                    diagnostic.message
                );
                let style = match diagnostic.severity {
                    Severity::Error => text::danger,
                    Severity::Warning => text::warning,
                    Severity::Info | Severity::Hint => muted,
                };
                col = col.push(text(label).size(12).font(Font::MONOSPACE).style(style));
            }
            if diagnostics.len() > MAX_DIAGNOSTICS_SHOWN {
                col = col.push(
                    text(format!(
                        "+{} more",
                        diagnostics.len() - MAX_DIAGNOSTICS_SHOWN
                    ))
                    .size(12)
                    .style(muted),
                );
            }
        }
        None => {
            if let Some(error) = &script.syntax_error {
                col = col.push(
                    text(format!("Syntax error: {error}"))
                        .size(12)
                        .font(Font::MONOSPACE)
                        .style(text::danger),
                );
            }
        }
    }

    if let Some(hover) = script.lsp.hover.as_deref().and_then(hover_summary) {
        col = col.push(text(hover).size(12).font(Font::MONOSPACE).style(muted));
    }

    col.into()
}

fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(muted_text_color(theme)),
    }
}

/// signature line plus the first line of the description.
fn hover_summary(hover: &str) -> Option<String> {
    let mut lines = hover.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    Some(match lines.next() {
        Some(second) => format!("{first}  —  {second}"),
        None => first.to_string(),
    })
}

fn key_binding<Message>(
    key_press: KeyPress,
    popup_open: bool,
    on_event: impl Fn(ScriptEditorEvent) -> Message,
) -> Option<Binding<Message>> {
    if !matches!(key_press.status, Status::Focused { .. }) {
        return None;
    }
    let custom = |event| Some(Binding::Custom(on_event(event)));
    let key = key_press.key.as_ref();

    let is_space = matches!(key, keyboard::Key::Named(Named::Space))
        || matches!(key, keyboard::Key::Character(" "));
    if is_space && key_press.modifiers.command() {
        return custom(ScriptEditorEvent::TriggerCompletion);
    }

    if popup_open {
        match key {
            keyboard::Key::Named(Named::ArrowUp) => {
                return custom(ScriptEditorEvent::CompletionMove(-1));
            }
            keyboard::Key::Named(Named::ArrowDown) => {
                return custom(ScriptEditorEvent::CompletionMove(1));
            }
            keyboard::Key::Named(Named::Enter | Named::Tab) => {
                return custom(ScriptEditorEvent::CompletionAccept);
            }
            keyboard::Key::Named(Named::Escape) => {
                return custom(ScriptEditorEvent::CompletionDismiss);
            }
            _ => {}
        }
    }

    if matches!(key, keyboard::Key::Named(Named::Tab)) && !key_press.modifiers.shift() {
        return Some(Binding::Sequence(vec![
            Binding::Insert(' '),
            Binding::Insert(' '),
        ]));
    }

    Binding::from_key_press(key_press)
}

fn completion_popup<'a, Message>(
    script: &'a ScriptContent,
    size: Size,
    on_event: impl Fn(ScriptEditorEvent) -> Message + Copy + 'a,
) -> Option<Element<'a, Message>>
where
    Message: Clone + 'a,
{
    let completion = script.shown_completion()?;
    let total = completion.filtered.len();
    let first = (completion.selected + 1).saturating_sub(POPUP_ROWS);
    let shown = first..(first + POPUP_ROWS).min(total);
    let row_count = shown.len();

    let mut list = column![].width(Length::Fill);
    for index in shown {
        let item = &completion.items[completion.filtered[index]];
        let is_selected = index == completion.selected;
        let detail = item.detail.as_deref().map(|d| truncate(d, 36));
        let entry = row![
            text(kind_label(item.kind))
                .size(11)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(18.0))
                .style(muted),
            text(item.label.as_str()).size(12).font(Font::MONOSPACE),
            space().width(Length::Fill),
            text(detail.unwrap_or_default()).size(11).style(muted),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        list = list.push(
            button(entry)
                .width(Length::Fill)
                .height(Length::Fixed(POPUP_ROW_HEIGHT))
                .padding([3, 8])
                .style(move |theme, status| {
                    if is_selected {
                        button::secondary(theme, status)
                    } else {
                        button::text(theme, status)
                    }
                })
                .on_press(on_event(ScriptEditorEvent::CompletionPick(index))),
        );
    }

    let selected = &completion.items[completion.filtered[completion.selected]];
    let doc = selected.documentation.as_deref().filter(|d| !d.is_empty());
    if let Some(doc) = doc {
        list = list.push(
            container(text(truncate(doc, 160)).size(11).style(muted))
                .padding([4, 8])
                .width(Length::Fill),
        );
    }

    let height =
        row_count as f32 * POPUP_ROW_HEIGHT + if doc.is_some() { POPUP_DOC_HEIGHT } else { 0.0 };
    let visible = script.visible_lines.get().max(1);
    let screen_row = (script.cursor_position().line as usize)
        .saturating_sub(script.top_line)
        .min(visible - 1) as f32;
    let below = PADDING + (screen_row + 1.0) * LINE_HEIGHT;
    let above = PADDING + screen_row * LINE_HEIGHT - height;
    let y = if below + height > size.height && above >= 0.0 {
        above
    } else {
        below
    };
    let anchor_chars = script.display_column(completion.anchor) - 1;
    let x = (PADDING + anchor_chars as f32 * CHAR_WIDTH - 26.0)
        .min(size.width - POPUP_WIDTH)
        .max(0.0);

    let popup = container(list)
        .width(Length::Fixed(POPUP_WIDTH))
        .padding(2)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.base.color.into()),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: 8.0,
                },
                ..container::Style::default()
            }
        });

    Some(pin(popup).x(x).y(y).into())
}

fn kind_label(kind: CompletionKind) -> &'static str {
    match kind {
        CompletionKind::Function | CompletionKind::Method => "ƒ",
        CompletionKind::Property => "◇",
        CompletionKind::Variable => "x",
        CompletionKind::Constant => "c",
        CompletionKind::Keyword => "k",
        CompletionKind::Module => "□",
        CompletionKind::Other => "·",
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.lines().next().unwrap_or_default();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_string(),
            kind: CompletionKind::Property,
            detail: None,
            documentation: None,
            insert_text: None,
        }
    }

    fn type_text(script: &mut ScriptContent, s: &str) {
        for c in s.chars() {
            script.update(ScriptEditorEvent::Action(Action::Edit(Edit::Insert(c))));
        }
    }

    #[test]
    fn filter_prefers_prefix_matches() {
        let items = vec![item("unset"), item("set"), item("setVariable"), item("get")];
        assert_eq!(filter_items(&items, "set"), vec![1, 2, 0]);
        assert_eq!(filter_items(&items, ""), vec![0, 1, 2, 3]);
    }

    #[test]
    fn dot_requests_completion_and_accept_replaces_prefix() {
        let mut script = ScriptContent::with_text("");
        type_text(&mut script, "pm.");
        assert!(script.lsp.want_completion);

        let anchor = script.word_start();
        script.open_completion(anchor, vec![item("environment"), item("expect")]);
        type_text(&mut script, "ex");
        let shown = script.shown_completion().expect("popup open");
        assert_eq!(shown.filtered.len(), 1);

        assert!(script.update(ScriptEditorEvent::CompletionAccept));
        assert_eq!(script.text(), "pm.expect");
        assert!(script.lsp.completion.is_none());
    }

    #[test]
    fn popup_closes_when_leaving_the_word() {
        let mut script = ScriptContent::with_text("");
        type_text(&mut script, "pm.");
        let anchor = script.word_start();
        script.open_completion(anchor, vec![item("test")]);
        type_text(&mut script, "t(");
        assert!(script.lsp.completion.is_none());
    }

    #[test]
    fn edits_bump_revision_and_keep_syntax_check() {
        let mut script = ScriptContent::with_text("");
        let before = script.revision();
        type_text(&mut script, "if (");
        assert!(script.revision() > before);
        assert!(script.syntax_error.is_some());
    }
}
