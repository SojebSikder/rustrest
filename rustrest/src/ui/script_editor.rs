//! JavaScript editor

use crate::ui::context_menu::with_context_menu;
use iced::widget::text_editor::Action;
use iced::widget::{column, container, text, text_editor};
use iced::{Element, Font, Length, Theme, highlighter};
use rustrest_core::script_engine::check_syntax;

const FONT_SIZE: f32 = 13.0;

#[derive(Debug, Clone)]
pub struct ScriptContent {
    content: text_editor::Content,
    error: Option<String>,
}

impl ScriptContent {
    pub fn with_text(text: &str) -> Self {
        Self {
            content: text_editor::Content::with_text(text),
            error: check_syntax(text).err(),
        }
    }

    pub fn perform(&mut self, action: Action) {
        let is_edit = action.is_edit();
        self.content.perform(action);
        if is_edit {
            self.error = check_syntax(&self.content.text()).err();
        }
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

/// Bordered, full-height script editor. `on_right_click` opens the context
/// menu; a syntax error, if any, is shown below the editor.
pub fn script_editor<'a, Message>(
    script: &'a ScriptContent,
    theme: &Theme,
    on_action: impl Fn(Action) -> Message + 'a,
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

    let editor = with_context_menu(
        text_editor(&script.content)
            .highlight("js", highlight_theme)
            .font(Font::MONOSPACE)
            .size(FONT_SIZE)
            .on_action(on_action)
            .height(Length::Fill)
            .padding(10),
        on_right_click,
    );

    let mut col = column![
        container(editor)
            .height(Length::Fill)
            .style(container::bordered_box)
    ]
    .spacing(6)
    .height(Length::Fill);

    if let Some(error) = &script.error {
        col = col.push(
            text(format!("Syntax error: {error}"))
                .size(12)
                .font(Font::MONOSPACE)
                .style(text::danger),
        );
    }

    col.into()
}
