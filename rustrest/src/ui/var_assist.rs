//! `{{variable}}` assistance for request fields: typing inside
//! `{{` pops up the matching variables, and hovering a `{{variable}}` shows
//! the value it resolves to and where it's defined.

use crate::collection::env::replace_vars_in;
use crate::ui::tab::types::KeyValuePair;
use iced::advanced::layout::{self, Layout};
use iced::advanced::text::{self as adv_text, Paragraph as _, Renderer as _};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::keyboard::key::{Key, Named};
use iced::widget::text_editor::{self, Action, Edit, Motion};
use iced::widget::text_input;
use iced::{
    Border, Color, Element, Event, Length, Padding, Pixels, Point, Rectangle, Renderer, Shadow,
    Size, Theme, Vector, alignment, keyboard, mouse,
};
use std::rc::Rc;
use std::sync::{Arc, LazyLock, RwLock};

type Paragraph = <Renderer as adv_text::Renderer>::Paragraph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarSource {
    Global,
    Collection,
    Environment,
}

impl VarSource {
    fn label(self) -> &'static str {
        match self {
            VarSource::Global => "GLOBAL",
            VarSource::Collection => "COLLECTION",
            VarSource::Environment => "ENVIRONMENT",
        }
    }

    fn badge(self) -> &'static str {
        match self {
            VarSource::Global => "G",
            VarSource::Collection => "C",
            VarSource::Environment => "E",
        }
    }

    fn color(self) -> Color {
        let colors = crate::theme::colors();
        match self {
            VarSource::Global => colors.info,
            VarSource::Collection => colors.warning,
            VarSource::Environment => colors.success,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    /// with nested `{{variables}}` already expanded
    pub value: String,
    pub source: VarSource,
}

static SCOPE: LazyLock<RwLock<Arc<Vec<Variable>>>> = LazyLock::new(Default::default);

/// makes these the variables fields offer and resolve, in Send's precedence:
/// environment over collection over globals.
pub fn publish_scope(globals: &[KeyValuePair], collection: &[KeyValuePair], env: &[KeyValuePair]) {
    let layers = [
        (globals, VarSource::Global),
        (collection, VarSource::Collection),
        (env, VarSource::Environment),
    ];
    let mut vars: Vec<Variable> = Vec::new();
    for (layer, source) in layers {
        for kv in layer.iter().filter(|kv| kv.is_active) {
            let name = kv.key.trim();
            if name.is_empty() {
                continue;
            }
            let value = if kv.value.contains("{{") {
                replace_vars_in(&kv.value, &[globals, collection, env])
            } else {
                kv.value.clone()
            };
            match vars.iter_mut().find(|v| v.name == name) {
                Some(existing) => {
                    existing.value = value;
                    existing.source = source;
                }
                None => vars.push(Variable {
                    name: name.to_string(),
                    value,
                    source,
                }),
            }
        }
    }
    vars.sort_by_key(|v| v.name.to_lowercase());

    let mut published = SCOPE.write().unwrap_or_else(|e| e.into_inner());
    if **published != vars {
        *published = Arc::new(vars);
    }
}

fn scope() -> Arc<Vec<Variable>> {
    SCOPE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// the `{{partial` the caret is in: `start` is the `{{`, `end` is the caret,
/// or past an existing `}}` closing the name the caret is inside.
#[derive(Debug, Clone, PartialEq)]
struct Completion {
    start: usize,
    end: usize,
    query: String,
}

fn is_name_break(c: char) -> bool {
    c == '{' || c == '}' || c.is_whitespace()
}

fn completion_at(line: &str, caret: usize) -> Option<Completion> {
    let before = line.get(..caret)?;
    let start = before.rfind("{{")?;
    let query = &before[start + 2..];
    if query.contains(is_name_break) {
        return None;
    }
    let after = &line[caret..];
    let name_tail = after.find(is_name_break).unwrap_or(after.len());
    let end = if after[name_tail..].starts_with("}}") {
        caret + name_tail + 2
    } else {
        caret
    };
    Some(Completion {
        start,
        end,
        query: query.to_string(),
    })
}

/// byte range and name of every `{{name}}` in `line`, matched the way
/// `replace_vars_in` matches them.
fn var_spans(line: &str) -> Vec<(usize, usize, &str)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(rel) = line[from..].find("{{") {
        let start = from + rel;
        let Some(close) = line[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + close + 2;
        let name = line[start + 2..end - 2].trim();
        if !name.is_empty() {
            spans.push((start, end, name));
        }
        from = end;
    }
    spans
}

/// indices into `scope` matching `query`, prefix matches first.
fn suggestions(scope: &[Variable], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let mut matches: Vec<usize> = (0..scope.len())
        .filter(|&i| scope[i].name.to_lowercase().contains(&query))
        .collect();
    matches.sort_by_key(|&i| !scope[i].name.to_lowercase().starts_with(&query));
    matches
}

/// a `text_input` offering `{{variable}}` completion and hover values.
pub fn var_input<'a, Message>(placeholder: &str, value: &str) -> VarInput<'a, Message> {
    VarInput {
        placeholder: placeholder.to_string(),
        value: value.to_string(),
        on_input: None,
        padding: text_input::DEFAULT_PADDING,
        width: Length::Fill,
        secure: false,
    }
}

pub struct VarInput<'a, Message> {
    placeholder: String,
    value: String,
    on_input: Option<Rc<dyn Fn(String) -> Message + 'a>>,
    padding: Padding,
    width: Length,
    secure: bool,
}

impl<'a, Message> VarInput<'a, Message> {
    pub fn on_input(mut self, on_input: impl Fn(String) -> Message + 'a) -> Self {
        self.on_input = Some(Rc::new(on_input));
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
}

impl<'a, Message: Clone + 'a> From<VarInput<'a, Message>> for Element<'a, Message> {
    fn from(input: VarInput<'a, Message>) -> Self {
        let mut widget = text_input(&input.placeholder, &input.value)
            .padding(input.padding)
            .width(input.width)
            .secure(input.secure);
        let Some(on_input) = input.on_input else {
            return widget.into();
        };
        let forward = on_input.clone();
        widget = widget.on_input(move |s| forward(s));

        Element::new(VarAssist {
            content: widget.into(),
            lines: vec![input.value],
            target: Target::Input {
                on_input,
                secure: input.secure,
            },
            padding: input.padding,
            scope: scope(),
        })
    }
}

/// wraps `editor` (a plain `text_editor` showing `content`, possibly behind a
/// context-menu wrapper) with `{{variable}}` completion and hover values.
pub fn var_editor<'a, Message: 'a>(
    editor: impl Into<Element<'a, Message>>,
    content: &text_editor::Content,
    padding: impl Into<Padding>,
    on_action: Rc<dyn Fn(Action) -> Message + 'a>,
) -> Element<'a, Message> {
    let cursor = content.cursor();
    let caret = cursor
        .selection
        .is_none()
        .then_some((cursor.position.line, cursor.position.column));

    Element::new(VarAssist {
        content: editor.into(),
        lines: content.lines().map(|l| l.text.into_owned()).collect(),
        target: Target::Editor { on_action, caret },
        padding: padding.into(),
        scope: scope(),
    })
}

enum Target<'a, Message> {
    Input {
        on_input: Rc<dyn Fn(String) -> Message + 'a>,
        secure: bool,
    },
    Editor {
        on_action: Rc<dyn Fn(Action) -> Message + 'a>,
        /// (line, byte column), `None` while text is selected
        caret: Option<(usize, usize)>,
    },
}

struct VarAssist<'a, Message> {
    content: Element<'a, Message>,
    lines: Vec<String>,
    target: Target<'a, Message>,
    padding: Padding,
    scope: Arc<Vec<Variable>>,
}

#[derive(Default)]
struct State {
    /// text as of the last diff, to tell typing apart from caret moves
    text: String,
    /// (line, start of `{{`) of the name the suggestions are open for
    open: Option<(usize, usize)>,
    query: String,
    selected: usize,
    /// first visible suggestion row
    scroll: usize,
    /// text_input caret to restore once an accepted suggestion lands
    pending_caret: Option<usize>,
    /// the hovered `{{variable}}`'s name and absolute bounds
    hovered: Option<(String, Rectangle)>,
}

impl State {
    fn select(&mut self, selected: usize) {
        self.selected = selected;
        if selected < self.scroll {
            self.scroll = selected;
        } else if selected >= self.scroll + MAX_ROWS {
            self.scroll = selected + 1 - MAX_ROWS;
        }
    }
}

fn find_state<T: 'static>(tree: &Tree) -> Option<&T> {
    if tree.tag == tree::Tag::of::<T>() {
        return Some(tree.state.downcast_ref::<T>());
    }
    tree.children.iter().find_map(find_state::<T>)
}

fn find_state_mut<T: 'static>(tree: &mut Tree) -> Option<&mut T> {
    if tree.tag == tree::Tag::of::<T>() {
        return Some(tree.state.downcast_mut::<T>());
    }
    tree.children.iter_mut().find_map(find_state_mut::<T>)
}

fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn paragraph(renderer: &Renderer, content: &str) -> Paragraph {
    Paragraph::with_text(adv_text::Text {
        content,
        bounds: Size::INFINITE,
        size: renderer.default_size(),
        line_height: adv_text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: adv_text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: adv_text::Shaping::Advanced,
        wrapping: adv_text::Wrapping::None,
    })
}

/// x of the byte offset `byte` within `line` as laid out in `paragraph`.
fn x_at(paragraph: &Paragraph, line: &str, byte: usize) -> f32 {
    let index = line.get(..byte).map_or(0, |s| s.chars().count());
    paragraph
        .grapheme_position(0, index)
        .map_or_else(|| paragraph.min_width(), |p| p.x)
}

impl<Message> VarAssist<'_, Message> {
    /// (line, byte column) of the caret, while focused with nothing selected.
    fn caret(&self, tree: &Tree) -> Option<(usize, usize)> {
        match &self.target {
            Target::Input { .. } => {
                let state = find_state::<text_input::State<Paragraph>>(tree)?;
                if !state.is_focused() {
                    return None;
                }
                let line = self.lines.first()?;
                match state.cursor().state(&text_input::Value::new(line)) {
                    text_input::cursor::State::Index(i) => Some((0, char_to_byte(line, i))),
                    text_input::cursor::State::Selection { .. } => None,
                }
            }
            Target::Editor { caret, .. } => {
                let state = find_state::<
                    text_editor::State<iced::advanced::text::highlighter::PlainText>,
                >(tree)?;
                if state.is_focused() { *caret } else { None }
            }
        }
    }

    /// the open suggestions: the caret's line, the name being typed and the
    /// matching variables.
    fn suggestions(&self, tree: &Tree) -> Option<(usize, Completion, Vec<usize>)> {
        let (line, column) = self.caret(tree)?;
        let completion = completion_at(self.lines.get(line)?, column)?;
        let state = tree.state.downcast_ref::<State>();
        if state.open != Some((line, completion.start)) {
            return None;
        }
        let matches = suggestions(&self.scope, &completion.query);
        (!matches.is_empty()).then_some((line, completion, matches))
    }

    /// what's drawn for `text`; a secure input shows one bullet per char.
    fn shown(&self, text: &str) -> String {
        match self.target {
            Target::Input { secure: true, .. } => "•".repeat(text.chars().count()),
            _ => text.to_string(),
        }
    }

    /// how far a focused text_input has scrolled its text left to keep the
    /// caret visible, mirroring iced's own computation.
    fn input_offset(&self, tree: &Tree, renderer: &Renderer, bounds: Rectangle) -> f32 {
        let Some(line) = self.lines.first() else {
            return 0.0;
        };
        let Some(state) = find_state::<text_input::State<Paragraph>>(tree) else {
            return 0.0;
        };
        if !state.is_focused() {
            return 0.0;
        }
        let index = match state.cursor().state(&text_input::Value::new(line)) {
            text_input::cursor::State::Index(i) => i,
            text_input::cursor::State::Selection { end, .. } => end,
        };
        let shown = self.shown(line);
        let caret_x = x_at(
            &paragraph(renderer, &shown),
            &shown,
            char_to_byte(&shown, index),
        );
        let text_width = bounds.width - self.padding.left - self.padding.right;
        (caret_x + 5.0 - text_width).max(0.0)
    }

    fn line_height(renderer: &Renderer) -> f32 {
        adv_text::LineHeight::default()
            .to_absolute(renderer.default_size())
            .0
    }

    /// the line's index and absolute (top, height) under `point`.
    fn line_at(
        &self,
        bounds: Rectangle,
        point: Point,
        renderer: &Renderer,
    ) -> Option<(usize, f32, f32)> {
        match self.target {
            Target::Input { .. } => Some((0, bounds.y, bounds.height)),
            Target::Editor { .. } => {
                let line_height = Self::line_height(renderer);
                let y = point.y - bounds.y - self.padding.top;
                if y < 0.0 {
                    return None;
                }
                let line = (y / line_height) as usize;
                let top = bounds.y + self.padding.top + line as f32 * line_height;
                Some((line, top, line_height))
            }
        }
    }

    /// colors every `{{variable}}` over the field's own text: the accent
    /// color when it resolves, the error color when it doesn't.
    fn draw_variables(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        // a secure input only shows bullets
        if matches!(self.target, Target::Input { secure: true, .. })
            || !self.lines.iter().any(|line| line.contains("{{"))
        {
            return;
        }
        let text_bounds = Rectangle {
            x: bounds.x + self.padding.left,
            y: bounds.y + self.padding.top,
            width: (bounds.width - self.padding.left - self.padding.right).max(0.0),
            height: (bounds.height - self.padding.top - self.padding.bottom).max(0.0),
        };
        let Some(clip) = text_bounds.intersection(viewport) else {
            return;
        };
        let offset = match self.target {
            Target::Input { .. } => self.input_offset(tree, renderer, bounds),
            Target::Editor { .. } => 0.0,
        };
        let line_height = Self::line_height(renderer);
        let colors = crate::theme::colors();

        // a layer of its own, so it lands over the field's text
        renderer.with_layer(clip, |renderer| {
            for (index, line) in self.lines.iter().enumerate() {
                // text_input centers its line, text_editor stacks them from the top
                let (text_y, align_y, chip_top) = match self.target {
                    Target::Input { .. } => {
                        let center = text_bounds.center_y();
                        (
                            center,
                            alignment::Vertical::Center,
                            center - line_height / 2.0,
                        )
                    }
                    Target::Editor { .. } => {
                        let top = text_bounds.y + index as f32 * line_height;
                        (top, alignment::Vertical::Top, top)
                    }
                };
                if chip_top > clip.y + clip.height || chip_top + line_height < clip.y {
                    continue;
                }
                let spans = var_spans(line);
                if spans.is_empty() {
                    continue;
                }
                let paragraph = paragraph(renderer, line);
                let origin = text_bounds.x - offset;
                for (start, end, name) in spans {
                    let color = if self.scope.iter().any(|v| v.name == name) {
                        colors.text_accent
                    } else {
                        colors.error
                    };
                    let x0 = origin + x_at(&paragraph, line, start);
                    let x1 = origin + x_at(&paragraph, line, end);
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: x0 - 1.0,
                                y: chip_top,
                                width: x1 - x0 + 2.0,
                                height: line_height,
                            },
                            border: Border {
                                radius: 3.0.into(),
                                ..Border::default()
                            },
                            ..renderer::Quad::default()
                        },
                        Color { a: 0.14, ..color },
                    );
                    renderer.fill_text(
                        adv_text::Text {
                            content: line[start..end].to_string(),
                            bounds: line_bounds(renderer.default_size().0),
                            size: renderer.default_size(),
                            line_height: adv_text::LineHeight::default(),
                            font: renderer.default_font(),
                            align_x: adv_text::Alignment::Left,
                            align_y,
                            shaping: adv_text::Shaping::Advanced,
                            wrapping: adv_text::Wrapping::None,
                        },
                        Point::new(x0, text_y),
                        color,
                        clip,
                    );
                }
            }
        });
    }

    /// the `{{variable}}` under `point`, with its absolute bounds.
    fn var_under(
        &self,
        tree: &Tree,
        bounds: Rectangle,
        point: Point,
        renderer: &Renderer,
    ) -> Option<(String, Rectangle)> {
        let (line_idx, top, height) = self.line_at(bounds, point, renderer)?;
        let line = self.lines.get(line_idx)?;
        let spans = var_spans(line);
        if spans.is_empty() {
            return None;
        }
        let offset = match self.target {
            Target::Input { .. } => self.input_offset(tree, renderer, bounds),
            Target::Editor { .. } => 0.0,
        };
        let shown = self.shown(line);
        let paragraph = paragraph(renderer, &shown);
        let origin = bounds.x + self.padding.left - offset;
        spans.into_iter().find_map(|(start, end, name)| {
            let to_shown = |byte: usize| char_to_byte(&shown, line[..byte].chars().count());
            let x0 = origin + x_at(&paragraph, &shown, to_shown(start));
            let x1 = origin + x_at(&paragraph, &shown, to_shown(end));
            (point.x >= x0 && point.x < x1).then(|| {
                (
                    name.to_string(),
                    Rectangle {
                        x: x0,
                        y: top,
                        width: x1 - x0,
                        height,
                    },
                )
            })
        })
    }

    /// absolute bounds of the `{{` the suggestions are for, to anchor them under.
    fn completion_anchor(
        &self,
        tree: &Tree,
        bounds: Rectangle,
        renderer: &Renderer,
        line_idx: usize,
        completion: &Completion,
    ) -> Rectangle {
        let line = self.lines.get(line_idx).map(String::as_str).unwrap_or("");
        let shown = self.shown(line);
        let start = char_to_byte(&shown, line[..completion.start].chars().count());
        let x = x_at(&paragraph(renderer, &shown), &shown, start);
        match self.target {
            Target::Input { .. } => {
                let offset = self.input_offset(tree, renderer, bounds);
                Rectangle {
                    x: bounds.x + self.padding.left + x - offset,
                    y: bounds.y,
                    width: 1.0,
                    height: bounds.height,
                }
            }
            Target::Editor { .. } => {
                let line_height = Self::line_height(renderer);
                Rectangle {
                    x: bounds.x + self.padding.left + x,
                    y: bounds.y + self.padding.top + line_idx as f32 * line_height,
                    width: 1.0,
                    height: line_height,
                }
            }
        }
    }

    /// opens the suggestions when typing lands inside a `{{`, and puts a text_input's
    /// caret after an accepted suggestion.
    fn sync(&self, tree: &mut Tree) {
        let text = self.lines.join("\n");
        let state = tree.state.downcast_mut::<State>();
        let changed = state.text != text;
        if changed {
            state.text = text;
            // the hovered variable may have moved or gone
            state.hovered = None;
        }
        let pending = state.pending_caret.take();
        if let (true, Some(caret), Target::Input { .. }) = (changed, pending, &self.target)
            && let Some(input) = find_state_mut::<text_input::State<Paragraph>>(tree)
        {
            input.move_cursor_to(caret);
        }

        let completion = self.caret(tree).and_then(|(line, column)| {
            completion_at(self.lines.get(line)?, column).map(|c| (line, c))
        });
        let state = tree.state.downcast_mut::<State>();
        match completion {
            Some((line, completion)) => {
                let key = Some((line, completion.start));
                if changed && state.open != key {
                    state.open = key;
                    state.query.clear();
                }
                if state.open == key && state.query != completion.query {
                    state.query = completion.query;
                    state.selected = 0;
                    state.scroll = 0;
                }
            }
            None => state.open = None,
        }
    }
}

/// replaces the `{{partial` with `{{name}}`.
fn accept<Message>(
    lines: &[String],
    target: &Target<'_, Message>,
    state: &mut State,
    line_idx: usize,
    completion: &Completion,
    name: &str,
    shell: &mut Shell<'_, Message>,
) {
    let Some(line) = lines.get(line_idx) else {
        return;
    };
    let replacement = format!("{{{{{name}}}}}");
    match target {
        Target::Input { on_input, .. } => {
            let text = format!(
                "{}{replacement}{}",
                &line[..completion.start],
                &line[completion.end..]
            );
            state.pending_caret =
                Some(line[..completion.start].chars().count() + replacement.chars().count());
            shell.publish(on_input(text));
        }
        Target::Editor { on_action, caret } => {
            let Some((_, column)) = caret else {
                return;
            };
            // select the whole partial name, then paste over it
            let right = line[*column..completion.end].chars().count();
            let left = line[completion.start..completion.end].chars().count();
            for _ in 0..right {
                shell.publish(on_action(Action::Move(Motion::Right)));
            }
            for _ in 0..left {
                shell.publish(on_action(Action::Select(Motion::Left)));
            }
            shell.publish(on_action(Action::Edit(Edit::Paste(Arc::new(replacement)))));
        }
    }
    state.open = None;
}

impl<'a, Message> iced::advanced::Widget<Message, Theme, Renderer> for VarAssist<'a, Message> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            text: self.lines.join("\n"),
            ..State::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
        self.sync(tree);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout(layout),
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // the open suggestions take the navigation keys from the field
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
            && let Some((line, completion, matches)) = self.suggestions(tree)
        {
            let state = tree.state.downcast_mut::<State>();
            let selected = state.selected.min(matches.len() - 1);
            let handled = match key.as_ref() {
                Key::Named(Named::ArrowDown) => {
                    state.select((selected + 1) % matches.len());
                    true
                }
                Key::Named(Named::ArrowUp) => {
                    state.select((selected + matches.len() - 1) % matches.len());
                    true
                }
                Key::Named(Named::Enter | Named::Tab) => {
                    let name = &self.scope[matches[selected]].name;
                    accept(
                        &self.lines,
                        &self.target,
                        state,
                        line,
                        &completion,
                        name,
                        shell,
                    );
                    true
                }
                Key::Named(Named::Escape) => {
                    state.open = None;
                    true
                }
                _ => false,
            };
            if handled {
                shell.request_redraw();
                shell.capture_event();
                return;
            }
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        if let Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) = event {
            let bounds = layout.bounds();
            let visible = bounds.intersection(viewport).unwrap_or(bounds);
            let hovered = cursor
                .position_over(visible)
                .and_then(|point| self.var_under(tree, bounds, point, renderer));
            let state = tree.state.downcast_mut::<State>();
            if state.hovered != hovered {
                state.hovered = hovered;
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child_layout(layout),
            cursor,
            viewport,
        );
        self.draw_variables(tree, renderer, layout.bounds(), viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout(layout),
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let bounds = layout.bounds();
        let kind = match self.suggestions(tree) {
            Some((line, completion, matches)) => {
                let anchor = self.completion_anchor(tree, bounds, renderer, line, &completion);
                Some((
                    anchor,
                    Popup::Suggestions {
                        line,
                        completion,
                        matches,
                    },
                ))
            }
            None => tree
                .state
                .downcast_ref::<State>()
                .hovered
                .clone()
                .map(|(name, anchor)| (anchor, Popup::Value { name })),
        };

        let Self {
            content,
            lines,
            target,
            scope,
            ..
        } = self;
        let Tree {
            state, children, ..
        } = tree;

        let mut overlays = Vec::new();
        if let Some(child) = content.as_widget_mut().overlay(
            &mut children[0],
            child_layout(layout),
            renderer,
            viewport,
            translation,
        ) {
            overlays.push(child);
        }
        if let Some((anchor, popup)) = kind {
            overlays.push(overlay::Element::new(Box::new(AssistOverlay {
                lines,
                target,
                scope,
                state: state.downcast_mut::<State>(),
                popup,
                anchor: anchor + translation,
            })));
        }

        match overlays.len() {
            0 => None,
            1 => overlays.pop(),
            _ => Some(overlay::Group::with_children(overlays).overlay()),
        }
    }
}

fn child_layout(layout: Layout<'_>) -> Layout<'_> {
    layout
        .children()
        .next()
        .expect("VarAssist always lays out its one child")
}

const MAX_ROWS: usize = 8;
const ROW_HEIGHT: f32 = 26.0;
const LIST_WIDTH: f32 = 360.0;
const NAME_WIDTH: f32 = 150.0;
const POPUP_PADDING: f32 = 4.0;
const TEXT_SIZE: f32 = 13.0;
const SMALL_TEXT_SIZE: f32 = 11.0;
const TOOLTIP_MAX_WIDTH: f32 = 420.0;
const TOOLTIP_LINE_HEIGHT: f32 = 18.0;
const MAX_VALUE_CHARS: usize = 80;
/// wider than any line drawn; see [`line_bounds`].
const LINE_BOUNDS_WIDTH: f32 = 10_000.0;

enum Popup {
    Suggestions {
        line: usize,
        completion: Completion,
        matches: Vec<usize>,
    },
    Value {
        name: String,
    },
}

struct AssistOverlay<'b, 'a, Message> {
    lines: &'b [String],
    target: &'b Target<'a, Message>,
    scope: &'b [Variable],
    state: &'b mut State,
    popup: Popup,
    /// absolute bounds of what the popup belongs to
    anchor: Rectangle,
}

fn truncate(text: &str, max_chars: usize) -> String {
    let single_line = text.replace(['\r', '\n'], " ");
    if single_line.chars().count() <= max_chars {
        single_line
    } else {
        let mut cut: String = single_line.chars().take(max_chars - 1).collect();
        cut.push('…');
        cut
    }
}

fn line_bounds(size: f32) -> Size {
    Size::new(
        LINE_BOUNDS_WIDTH,
        adv_text::LineHeight::default().to_absolute(Pixels(size)).0,
    )
}

fn label(content: String, size: f32, renderer: &Renderer) -> adv_text::Text<String, iced::Font> {
    adv_text::Text {
        content,
        bounds: line_bounds(size),
        size: Pixels(size),
        line_height: adv_text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: adv_text::Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: adv_text::Shaping::Advanced,
        wrapping: adv_text::Wrapping::None,
    }
}

fn text_width(content: &str, size: f32, renderer: &Renderer) -> f32 {
    Paragraph::with_text(adv_text::Text {
        content,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: adv_text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: adv_text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: adv_text::Shaping::Advanced,
        wrapping: adv_text::Wrapping::None,
    })
    .min_width()
}

impl<Message> AssistOverlay<'_, '_, Message> {
    /// the hovered variable's tooltip lines: (header, header color, value, value color)
    fn value_lines(&self, name: &str) -> (String, Color, String, Color) {
        let colors = crate::theme::colors();
        match self.scope.iter().find(|v| v.name == name) {
            Some(var) if var.value.is_empty() => (
                var.source.label().to_string(),
                var.source.color(),
                "(empty)".to_string(),
                colors.text_muted,
            ),
            Some(var) => (
                var.source.label().to_string(),
                var.source.color(),
                truncate(&var.value, MAX_VALUE_CHARS),
                colors.text,
            ),
            None => (
                "UNRESOLVED VARIABLE".to_string(),
                colors.error,
                "Not defined in the active environment, collection or globals".to_string(),
                colors.text_muted,
            ),
        }
    }

    fn row_at(&self, layout: Layout<'_>, point: Point) -> Option<usize> {
        let Popup::Suggestions { matches, .. } = &self.popup else {
            return None;
        };
        let bounds = layout.bounds();
        if !bounds.contains(point) {
            return None;
        }
        let row = ((point.y - bounds.y - POPUP_PADDING) / ROW_HEIGHT).floor();
        let index = self.state.scroll + row.max(0.0) as usize;
        (index < matches.len()).then_some(index)
    }
}

fn panel(renderer: &mut Renderer, bounds: Rectangle) {
    use iced::advanced::Renderer as _;
    let colors = crate::theme::colors();
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: Border {
                radius: 6.0.into(),
                width: 1.0,
                color: colors.border,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
            ..renderer::Quad::default()
        },
        colors.elevated_surface_background,
    );
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for AssistOverlay<'_, '_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let size = match &self.popup {
            Popup::Suggestions { matches, .. } => Size::new(
                LIST_WIDTH,
                matches.len().min(MAX_ROWS) as f32 * ROW_HEIGHT + POPUP_PADDING * 2.0,
            ),
            Popup::Value { name } => {
                let (header, _, value, _) = self.value_lines(name);
                let content_width = text_width(&header, SMALL_TEXT_SIZE, renderer)
                    .max(text_width(&value, TEXT_SIZE, renderer));
                Size::new(
                    (content_width + 20.0).clamp(140.0, TOOLTIP_MAX_WIDTH),
                    TOOLTIP_LINE_HEIGHT * 2.0 + 12.0,
                )
            }
        };

        // below the anchor, or above it when there's no room beneath
        let below = self.anchor.y + self.anchor.height + 4.0;
        let y = if below + size.height > bounds.height && self.anchor.y - size.height - 4.0 >= 0.0 {
            self.anchor.y - size.height - 4.0
        } else {
            below
        };
        let x = self.anchor.x.min(bounds.width - size.width).max(0.0);
        layout::Node::new(size).move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        use iced::advanced::Renderer as _;
        let colors = crate::theme::colors();
        let bounds = layout.bounds();
        panel(renderer, bounds);

        match &self.popup {
            Popup::Suggestions { matches, .. } => {
                let visible = matches
                    .iter()
                    .enumerate()
                    .skip(self.state.scroll)
                    .take(MAX_ROWS);
                for (row, (index, &var_index)) in visible.enumerate() {
                    let var = &self.scope[var_index];
                    let row_bounds = Rectangle {
                        x: bounds.x + POPUP_PADDING,
                        y: bounds.y + POPUP_PADDING + row as f32 * ROW_HEIGHT,
                        width: bounds.width - POPUP_PADDING * 2.0,
                        height: ROW_HEIGHT,
                    };
                    if index == self.state.selected {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: row_bounds,
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Border::default()
                                },
                                ..renderer::Quad::default()
                            },
                            colors.element_selected,
                        );
                    }
                    let center_y = row_bounds.center_y();

                    // source badge
                    let badge = Rectangle {
                        x: row_bounds.x + 6.0,
                        y: center_y - 8.0,
                        width: 16.0,
                        height: 16.0,
                    };
                    let source_color = var.source.color();
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: badge,
                            border: Border {
                                radius: 3.0.into(),
                                ..Border::default()
                            },
                            ..renderer::Quad::default()
                        },
                        Color {
                            a: 0.2,
                            ..source_color
                        },
                    );
                    renderer.fill_text(
                        adv_text::Text {
                            align_x: adv_text::Alignment::Center,
                            ..label(var.source.badge().to_string(), SMALL_TEXT_SIZE, renderer)
                        },
                        Point::new(badge.center_x(), center_y),
                        source_color,
                        badge,
                    );

                    let name_x = badge.x + badge.width + 8.0;
                    let name_clip = Rectangle {
                        x: name_x,
                        width: NAME_WIDTH,
                        ..row_bounds
                    };
                    renderer.fill_text(
                        label(var.name.clone(), TEXT_SIZE, renderer),
                        Point::new(name_x, center_y),
                        colors.text,
                        name_clip,
                    );

                    let value_x = name_x + NAME_WIDTH + 8.0;
                    let value_clip = Rectangle {
                        x: value_x,
                        width: (row_bounds.x + row_bounds.width - value_x - 6.0).max(0.0),
                        ..row_bounds
                    };
                    renderer.fill_text(
                        label(
                            truncate(&var.value, MAX_VALUE_CHARS),
                            SMALL_TEXT_SIZE + 1.0,
                            renderer,
                        ),
                        Point::new(value_x, center_y),
                        colors.text_muted,
                        value_clip,
                    );
                }
            }
            Popup::Value { name } => {
                let (header, header_color, value, value_color) = self.value_lines(name);
                let x = bounds.x + 10.0;
                let top = bounds.y + 6.0;
                renderer.fill_text(
                    label(header, SMALL_TEXT_SIZE, renderer),
                    Point::new(x, top + TOOLTIP_LINE_HEIGHT / 2.0),
                    header_color,
                    bounds,
                );
                renderer.fill_text(
                    label(value, TEXT_SIZE, renderer),
                    Point::new(x, top + TOOLTIP_LINE_HEIGHT * 1.5),
                    value_color,
                    bounds,
                );
            }
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let Popup::Suggestions {
            line,
            completion,
            matches,
        } = &self.popup
        else {
            return;
        };
        let over = cursor.is_over(layout.bounds());

        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(row) = self.row_at(layout, *position)
                    && row != self.state.selected
                {
                    self.state.selected = row;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
                let lines = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y,
                    mouse::ScrollDelta::Pixels { y, .. } => -y / ROW_HEIGHT,
                };
                let max_scroll = matches.len().saturating_sub(MAX_ROWS) as f32;
                let scroll = (self.state.scroll as f32 + lines.round()).clamp(0.0, max_scroll);
                self.state.scroll = scroll as usize;
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                if let Some(row) = cursor.position().and_then(|p| self.row_at(layout, p)) {
                    let name = self.scope[matches[row]].name.clone();
                    let (line, completion) = (*line, completion.clone());
                    accept(
                        self.lines,
                        self.target,
                        self.state,
                        line,
                        &completion,
                        &name,
                        shell,
                    );
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        match self.popup {
            Popup::Suggestions { .. } if cursor.is_over(layout.bounds()) => {
                mouse::Interaction::Pointer
            }
            _ => mouse::Interaction::None,
        }
    }
}
