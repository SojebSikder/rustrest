use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{Id, button, column, container, scrollable, text, text_input};
use iced::{Alignment, Color, Element, Font, Length};
use rustrest_command_palette::{Command, PaletteState, filter};

/// every action reachable from the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    NewRequestTab,
    NewTerminal,
    ImportCollection,
    ImportGitFolder,
    CreateNewCollection,
    ToggleConsolePanel,
    CheckForUpdate,
    RemoteDevelopmentOverSsh,
}

pub fn commands() -> Vec<Command<AppCommand>> {
    vec![
        Command::new(
            "new-request-tab",
            "New Request Tab",
            AppCommand::NewRequestTab,
        ),
        Command::new("new-terminal", "New Terminal", AppCommand::NewTerminal)
            .with_subtitle("Ctrl+`"),
        Command::new(
            "import-collection",
            "Import Collection...",
            AppCommand::ImportCollection,
        ),
        Command::new(
            "import-git-folder",
            "Import Git Folder...",
            AppCommand::ImportGitFolder,
        ),
        Command::new(
            "create-collection",
            "Create New Collection",
            AppCommand::CreateNewCollection,
        ),
        Command::new(
            "toggle-console-panel",
            "Toggle Console Panel",
            AppCommand::ToggleConsolePanel,
        ),
        Command::new(
            "check-for-update",
            "Check for Updates",
            AppCommand::CheckForUpdate,
        ),
        Command::new(
            "remote-development-over-ssh",
            "Remote Development over SSH",
            AppCommand::RemoteDevelopmentOverSsh,
        )
        .with_subtitle("Configure saved hosts in a new window"),
    ]
}

/// maps a palette selection to the real application message that performs it.
pub fn to_message(action: AppCommand) -> Message {
    match action {
        AppCommand::NewRequestTab => Message::NewTabPressed,
        AppCommand::NewTerminal => Message::NewTerminalTabPressed,
        AppCommand::ImportCollection => Message::ImportCollectionPressed,
        AppCommand::ImportGitFolder => Message::ImportGitCollectionPressed,
        AppCommand::CreateNewCollection => Message::CreateNewCollectionPressed,
        AppCommand::ToggleConsolePanel => Message::ToggleConsolePanel,
        AppCommand::CheckForUpdate => Message::CheckForUpdate,
        AppCommand::RemoteDevelopmentOverSsh => Message::OpenRemoteConfigWindow,
    }
}

/// stable widget id so opening the palette can immediately focus its search box.
pub fn input_id() -> Id {
    Id::new("rustrest-command-palette-input")
}

/// the currently-matching commands for `state.query`, in display order.
pub fn matches_for(state: &PaletteState) -> Vec<Command<AppCommand>> {
    let commands = commands();
    filter(&commands, &state.query)
        .into_iter()
        .copied()
        .collect()
}

pub fn view(state: &PaletteState) -> Element<'_, Message> {
    let matches = matches_for(state);

    let input = text_input("Type a command...", &state.query)
        .id(input_id())
        .on_input(Message::CommandPaletteQueryChanged)
        .on_submit(Message::CommandPaletteConfirm)
        .padding(10)
        .size(15);

    let mut list = column![].spacing(2);
    if matches.is_empty() {
        list = list.push(
            container(
                text("No matching commands")
                    .size(12)
                    .style(|_theme: &iced::Theme| text::Style {
                        color: Some(Color::from_rgb(0.55, 0.55, 0.6)),
                    }),
            )
            .padding(8),
        );
    }
    for (idx, cmd) in matches.iter().enumerate() {
        let is_selected = idx == state.selected;

        let mut row_content = column![text(cmd.title).size(13)].spacing(2);
        if let Some(subtitle) = cmd.subtitle {
            row_content =
                row_content.push(text(subtitle).size(11).style(|_theme: &iced::Theme| {
                    text::Style {
                        color: Some(Color::from_rgb(0.55, 0.55, 0.6)),
                    }
                }));
        }

        let action = cmd.action;
        list = list.push(
            button(row_content)
                .on_press(Message::CommandPaletteItemClicked(action))
                .width(Length::Fill)
                .padding(8)
                .style(move |theme, status| {
                    if is_selected {
                        button::Style {
                            background: Some(theme.extended_palette().primary.weak.color.into()),
                            text_color: theme.extended_palette().primary.weak.text,
                            ..button::text(theme, status)
                        }
                    } else {
                        button::text(theme, status)
                    }
                }),
        );
    }

    let title = text("Command Palette").size(14).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let body = column![title, input, scrollable(list).height(Length::Fixed(320.0)),]
        .spacing(10)
        .padding(20)
        .align_x(Alignment::Start);

    card(body, 480.0)
}
