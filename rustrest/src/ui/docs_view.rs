//! reusable markdown documentation panes.
//!
//! `MarkdownDoc` + `markdown_doc_view` is the editor/preview pair every doc
//! surface shares: the request tab's Docs sub-tab embeds it directly, while
//! `DocsState` + `view` wrap it with generated docs and export for the
//! collection tab's Docs sub-tab and the folder tab.

use crate::collection::collection::PostmanCollection;
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use iced::widget::{
    button, checkbox, column, container, markdown, row, scrollable, space, text, text_editor,
};
use iced::{Alignment, Element, Length, Theme};
use rustrest_core::docs::{self, DocsOptions, DocsTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsMode {
    /// markdown source editor with a live preview beside it.
    Edit,
    /// just the rendered description.
    Preview,
    /// the full generated docs for the target and everything under it.
    Generated,
}

impl DocsMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Preview => "Preview",
            Self::Generated => "Generated Docs",
        }
    }
}

/// a markdown description being edited, plus its rendered preview.
#[derive(Debug, Clone)]
pub struct MarkdownDoc {
    pub mode: DocsMode,
    pub editor: text_editor::Content,
    preview: Vec<markdown::Item>,
}

impl MarkdownDoc {
    pub fn new(markdown_text: &str) -> Self {
        Self {
            // an empty description has nothing to preview; start in the editor
            mode: if markdown_text.trim().is_empty() {
                DocsMode::Edit
            } else {
                DocsMode::Preview
            },
            editor: text_editor::Content::with_text(markdown_text),
            preview: markdown::parse(markdown_text).collect(),
        }
    }

    /// applies an editor action; returns `true` if it changed the text.
    pub fn perform(&mut self, action: text_editor::Action) -> bool {
        let is_edit = action.is_edit();
        self.editor.perform(action);
        if is_edit {
            self.preview = markdown::parse(&self.editor.text()).collect();
        }
        is_edit
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    /// the description as stored on a collection node (blank means none).
    pub fn description(&self) -> Option<String> {
        let text = self.text();
        (!text.trim().is_empty()).then_some(text)
    }
}

/// a row of mode toggle buttons.
pub fn mode_bar<'a, Message: Clone + 'a>(
    current: DocsMode,
    modes: &[DocsMode],
    on_select: impl Fn(DocsMode) -> Message,
) -> Element<'a, Message> {
    let mut bar = row![].spacing(6).align_y(Alignment::Center);
    for &mode in modes {
        let btn = button(text(mode.label()).size(12)).padding([4, 10]);
        bar = bar.push(if mode == current {
            btn.style(button::primary)
        } else {
            btn.style(button::text).on_press(on_select(mode))
        });
    }
    bar.into()
}

/// renders parsed markdown in a scrollable pane; right-clicking it sends
/// `on_right_click` (a Copy menu, since rendered text isn't selectable).
pub fn rendered_markdown<'a, Message: Clone + 'a>(
    items: &'a [markdown::Item],
    theme: &Theme,
    on_link: impl Fn(markdown::Uri) -> Message + 'a,
    on_right_click: Message,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if items.is_empty() {
        text("No documentation yet. Switch to Edit to write some in Markdown.")
            .size(13)
            .style(text::secondary)
            .into()
    } else {
        markdown::view(items, theme).map(on_link)
    };
    with_context_menu(
        scrollable(container(body).padding(12).width(Length::Fill)).height(Length::Fill),
        on_right_click,
    )
}

/// the editor (with side-by-side preview) in `Edit` mode, or just the
/// rendered description otherwise. `on_context_menu` opens the right-click
/// menu for the editor (`FieldTarget::…DocsEditor`) or the preview
/// (`…DocsPreview`, carrying the text to copy).
pub fn markdown_doc_view<'a, Message: Clone + 'a>(
    doc: &'a MarkdownDoc,
    theme: &Theme,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
    on_link: impl Fn(markdown::Uri) -> Message + 'a,
    on_context_menu: impl Fn(DocsField, String) -> Message,
) -> Element<'a, Message> {
    let preview = container(rendered_markdown(
        &doc.preview,
        theme,
        on_link,
        on_context_menu(DocsField::Preview, doc.text()),
    ))
    .style(container::bordered_box)
    .width(Length::FillPortion(1))
    .height(Length::Fill);

    if doc.mode != DocsMode::Edit {
        return preview.into();
    }

    let editor = text_editor(&doc.editor)
        .placeholder("Write documentation in Markdown…")
        .on_action(on_action)
        .padding(10)
        .height(Length::Fill);
    // the editor's live selection is looked up when the menu renders
    let editor = with_context_menu(editor, on_context_menu(DocsField::Editor, String::new()));
    row![
        container(editor)
            .style(container::bordered_box)
            .width(Length::FillPortion(1))
            .height(Length::Fill),
        preview,
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}

/// which part of a docs pane was right-clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsField {
    Editor,
    Preview,
}

#[derive(Debug, Clone)]
pub enum DocsMessage {
    ShowContextMenu(FieldTarget, String),
    ModeSelected(DocsMode),
    EditorAction(text_editor::Action),
    OptionsChanged(DocsOptions),
    LinkClicked(markdown::Uri),
    CopyMarkdown,
    ExportMarkdown,
}

/// docs for a collection or folder: its own description plus generated docs
/// covering everything beneath it.
#[derive(Debug, Clone)]
pub struct DocsState {
    pub collection_id: usize,
    pub target: DocsTarget,
    pub doc: MarkdownDoc,
    pub options: DocsOptions,
    generated: String,
    generated_preview: Vec<markdown::Item>,
}

impl DocsState {
    /// loads `target`'s current description from `collection` and renders
    /// both previews.
    pub fn new(collection: &PostmanCollection, target: DocsTarget) -> Self {
        let description = docs::description(collection, &target).unwrap_or_default();
        let mut state = Self {
            collection_id: collection.id,
            doc: MarkdownDoc::new(description),
            target,
            options: DocsOptions::default(),
            generated: String::new(),
            generated_preview: Vec::new(),
        };
        state.regenerate(collection);
        state
    }

    pub fn generated_markdown(&self) -> &str {
        &self.generated
    }

    /// rebuilds the generated docs from the collection's current contents.
    pub fn regenerate(&mut self, collection: &PostmanCollection) {
        self.generated =
            docs::generate(collection, &self.target, &self.options).unwrap_or_default();
        self.generated_preview = markdown::parse(&self.generated).collect();
    }
}

pub fn view<'a, Message: Clone + 'a>(
    state: &'a DocsState,
    collection: Option<&'a PostmanCollection>,
    theme: &Theme,
    wrap: impl Fn(DocsMessage) -> Message + Copy + 'static,
) -> Element<'a, Message> {
    if collection
        .and_then(|c| docs::target_name(c, &state.target))
        .is_none()
    {
        return container(text("This item no longer exists.").size(14))
            .padding(20)
            .into();
    }

    let toolbar = row![
        mode_bar(
            state.doc.mode,
            &[DocsMode::Edit, DocsMode::Preview, DocsMode::Generated],
            move |mode| wrap(DocsMessage::ModeSelected(mode)),
        ),
        space::horizontal(),
        button(text("Copy Markdown").size(12))
            .padding([4, 10])
            .style(button::secondary)
            .on_press(wrap(DocsMessage::CopyMarkdown)),
        button(text("Export .md").size(12))
            .padding([4, 10])
            .style(button::primary)
            .on_press(wrap(DocsMessage::ExportMarkdown)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let on_link = move |uri| wrap(DocsMessage::LinkClicked(uri));
    let content: Element<'a, Message> = if state.doc.mode == DocsMode::Generated {
        let options = state.options;
        let toggle = |label: &'static str, value: bool, apply: fn(&mut DocsOptions, bool)| {
            checkbox(value)
                .label(label)
                .text_size(12)
                .on_toggle(move |v| {
                    let mut next = options;
                    apply(&mut next, v);
                    wrap(DocsMessage::OptionsChanged(next))
                })
        };
        column![
            row![
                toggle("Table of contents", options.include_toc, |o, v| o
                    .include_toc =
                    v),
                toggle("Example responses", options.include_examples, |o, v| o
                    .include_examples =
                    v),
                toggle("Scripts", options.include_scripts, |o, v| o
                    .include_scripts =
                    v),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
            container(rendered_markdown(
                &state.generated_preview,
                theme,
                on_link,
                wrap(DocsMessage::ShowContextMenu(
                    FieldTarget::DocsPreview,
                    state.generated.clone(),
                )),
            ))
            .style(container::bordered_box)
            .height(Length::Fill),
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    } else {
        markdown_doc_view(
            &state.doc,
            theme,
            move |action| wrap(DocsMessage::EditorAction(action)),
            on_link,
            move |field, value| {
                let target = match field {
                    DocsField::Editor => FieldTarget::DocsEditor,
                    DocsField::Preview => FieldTarget::DocsPreview,
                };
                wrap(DocsMessage::ShowContextMenu(target, value))
            },
        )
    };

    column![toolbar, content]
        .spacing(12)
        .height(Length::Fill)
        .into()
}
