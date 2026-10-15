//! "Generate Code" modal: turns the active request into a snippet for any
//! target in `rustrest_core::codegen`, with that target's options.

use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use crate::ui::tooltip::with_tooltip;
use iced::widget::tooltip::Position;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_editor};
use iced::{Alignment, Element, Font, Length, Theme};
use rustrest_core::codegen::{self, CodeGenerator, CodegenRequest, OptionValues};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum CodegenMessage {
    LanguageSelected(Language),
    OptionToggled(&'static str, bool),
    ResolveVariablesToggled(bool),
    EditorAction(text_editor::Action),
    Copy,
    Close,
}

/// the user's picks, kept across opens for the session.
#[derive(Debug, Clone)]
pub struct CodegenPrefs {
    pub generator_id: &'static str,
    pub resolve_variables: bool,
    /// per generator id
    pub options: HashMap<&'static str, OptionValues>,
}

impl Default for CodegenPrefs {
    fn default() -> Self {
        Self {
            generator_id: codegen::generators()[0].id(),
            resolve_variables: true,
            options: HashMap::new(),
        }
    }
}

impl CodegenPrefs {
    pub fn generator(&self) -> &'static dyn CodeGenerator {
        codegen::find(self.generator_id).unwrap_or(codegen::generators()[0])
    }

    pub fn option_values(&self) -> OptionValues {
        self.options
            .get(self.generator_id)
            .cloned()
            .unwrap_or_default()
    }
}

pub struct CodegenModalState {
    pub request_name: String,
    /// with environment, collection and global variables substituted.
    pub resolved: Result<CodegenRequest, String>,
    /// with `{{var}}` placeholders left as typed.
    pub unresolved: Result<CodegenRequest, String>,
    /// read-only editor holding the generated snippet, so it can be selected.
    pub code: text_editor::Content,
}

impl CodegenModalState {
    pub fn new(
        request_name: String,
        resolved: Result<CodegenRequest, String>,
        unresolved: Result<CodegenRequest, String>,
        prefs: &CodegenPrefs,
    ) -> Self {
        let mut state = Self {
            request_name,
            resolved,
            unresolved,
            code: text_editor::Content::new(),
        };
        state.regenerate(prefs);
        state
    }

    fn request(&self, prefs: &CodegenPrefs) -> &Result<CodegenRequest, String> {
        if prefs.resolve_variables {
            &self.resolved
        } else {
            &self.unresolved
        }
    }

    /// the snippet for the current prefs, or why the request can't be built.
    pub fn snippet(&self, prefs: &CodegenPrefs) -> Result<String, String> {
        self.request(prefs)
            .as_ref()
            .map(|req| prefs.generator().generate(req, &prefs.option_values()))
            .map_err(Clone::clone)
    }

    pub fn regenerate(&mut self, prefs: &CodegenPrefs) {
        let code = self.snippet(prefs).unwrap_or_default();
        self.code = text_editor::Content::with_text(&code);
    }
}

/// a picker entry; `codegen::CodeGenerator` itself isn't `PartialEq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub id: &'static str,
    pub label: &'static str,
}

impl From<&'static dyn CodeGenerator> for Language {
    fn from(generator: &'static dyn CodeGenerator) -> Self {
        Self {
            id: generator.id(),
            label: generator.label(),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

fn wrap(msg: CodegenMessage) -> Message {
    Message::Codegen(msg)
}

fn muted(label: impl text::IntoFragment<'static>) -> iced::widget::Text<'static> {
    text(label).size(12).style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    })
}

fn field_label(label: &'static str) -> iced::widget::Text<'static> {
    text(label).size(12).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    })
}

pub fn view_codegen_modal<'a>(
    state: &'a CodegenModalState,
    prefs: &'a CodegenPrefs,
) -> Element<'a, Message> {
    let generator = prefs.generator();
    let values = prefs.option_values();

    let title = column![
        text("Generate Code").size(18).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        }),
        muted(format!(
            "Snippet for \u{201c}{}\u{201d}",
            state.request_name
        )),
    ]
    .spacing(2);

    let languages: Vec<Language> = codegen::generators()
        .iter()
        .map(|g| Language::from(*g))
        .collect();
    let language_picker = column![
        field_label("Language"),
        pick_list(languages, Some(Language::from(generator)), |lang| {
            wrap(CodegenMessage::LanguageSelected(lang))
        })
        .text_size(13)
        .padding([6, 10])
        .width(Length::Fixed(200.0)),
    ]
    .spacing(6);

    let resolve_toggle = with_tooltip(
        checkbox(prefs.resolve_variables)
            .label("Resolve variables")
            .text_size(12)
            .on_toggle(|v| wrap(CodegenMessage::ResolveVariablesToggled(v))),
        "Substitute environment, collection and global {{variables}}",
        Position::Bottom,
    );

    let top_row = row![
        language_picker,
        iced::widget::space::horizontal(),
        resolve_toggle,
    ]
    .align_y(Alignment::End);

    let mut option_row = row![].spacing(16).align_y(Alignment::Center);
    for option in generator.options() {
        let key = option.key;
        option_row = option_row.push(with_tooltip(
            checkbox(values.get(option))
                .label(option.label)
                .text_size(12)
                .on_toggle(move |v| wrap(CodegenMessage::OptionToggled(key, v))),
            option.description,
            Position::Bottom,
        ));
    }
    let options_section = column![field_label("Options"), option_row.wrap()].spacing(6);

    let snippet = state.snippet(prefs);
    let code_view: Element<'a, Message> = match &snippet {
        Ok(_) => text_editor(&state.code)
            .on_action(|action| wrap(CodegenMessage::EditorAction(action)))
            .highlight_with::<crate::theme::syntax::Highlighter>(
                crate::theme::syntax::Settings {
                    theme: crate::theme::syntax(),
                    token: generator.syntax().to_string(),
                },
                crate::theme::syntax::Highlight::to_format,
            )
            .font(Font::MONOSPACE)
            .size(13)
            .padding(12)
            .height(Length::Fixed(340.0))
            .into(),
        Err(e) => container(
            text(format!("Couldn't build the request: {e}"))
                .size(13)
                .style(|_: &Theme| text::Style {
                    color: Some(crate::theme::colors().error),
                }),
        )
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fixed(340.0))
        .style(|theme: &Theme| {
            let mut s = container::bordered_box(theme);
            s.border.radius = 6.0.into();
            s
        })
        .into(),
    };

    let copy_btn = button(text("Copy").size(14))
        .on_press_maybe(snippet.is_ok().then_some(wrap(CodegenMessage::Copy)))
        .padding([8, 16])
        .style(button::primary);
    let close_btn = button(text("Close").size(14))
        .on_press(wrap(CodegenMessage::Close))
        .padding([8, 16])
        .style(button::secondary);

    let footer = row![
        muted("Pre-request scripts and plugin hooks aren't applied."),
        iced::widget::space::horizontal(),
        copy_btn,
        close_btn,
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let body = column![title, top_row, options_section, code_view, footer]
        .spacing(16)
        .padding(24);

    card(body, 760.0)
}
