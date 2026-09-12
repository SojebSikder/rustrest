use crate::collection::git_ops::{GitChangeKind, GitFileEntry, GitStatusSnapshot};
use crate::message::Message;
use iced::widget::{Space, button, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Color, Element, Font, Length};
use std::path::PathBuf;

/// small colored letter badge for a file's git status, mirroring the log
/// level badges in `console_panel.rs`. Shared with the commit modal's file list.
pub fn status_badge(status: GitChangeKind) -> Element<'static, Message> {
    let (label, color) = match status {
        GitChangeKind::Untracked => ("U", Color::from_rgb(0.45, 0.45, 0.45)),
        GitChangeKind::Modified => ("M", Color::from_rgb(0.85, 0.55, 0.10)),
        GitChangeKind::Added => ("A", Color::from_rgb(0.25, 0.65, 0.35)),
        GitChangeKind::Deleted => ("D", Color::from_rgb(0.87, 0.22, 0.22)),
        GitChangeKind::Renamed => ("R", Color::from_rgb(0.20, 0.45, 0.85)),
        GitChangeKind::Conflicted => ("!", Color::from_rgb(0.87, 0.22, 0.22)),
    };

    container(
        text(label)
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::WHITE),
    )
    .padding([2, 6])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(color)),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn file_row(
    collection_id: usize,
    entry: &GitFileEntry,
    is_selected: bool,
) -> Element<'static, Message> {
    let path_str = entry.path.display().to_string();
    let entry_path = entry.path.clone();

    let content = row![
        status_badge(entry.status),
        text(path_str).size(13).font(Font::MONOSPACE),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    mouse_area(
        container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| {
                if is_selected {
                    container::Style {
                        background: Some(Color::from_rgb(0.20, 0.22, 0.28).into()),
                        ..Default::default()
                    }
                } else {
                    container::Style::default()
                }
            }),
    )
    .on_press(Message::GitDiffRequested(collection_id, entry_path))
    .into()
}

/// header strip for the git sub-tab: branch name + refresh + commit buttons.
pub fn render_git_bar(
    collection_id: usize,
    snapshot: Option<&Result<GitStatusSnapshot, String>>,
) -> Element<'static, Message> {
    let branch_label = match snapshot {
        Some(Ok(s)) => s
            .branch
            .clone()
            .unwrap_or_else(|| "(no branch)".to_string()),
        _ => "-".to_string(),
    };
    let change_count = match snapshot {
        Some(Ok(s)) => s.files.len(),
        _ => 0,
    };

    row![
        text(format!("Branch: {branch_label}")).size(13),
        Space::new().width(Length::Fill),
        button(text("Refresh").size(12))
            .style(button::text)
            .padding([4, 8])
            .on_press(Message::GitStatusRequested(collection_id)),
        button(text("Commit...").size(12))
            .style(button::primary)
            .padding([4, 8])
            .on_press_maybe(
                (change_count > 0).then_some(Message::CommitChangesPressed(collection_id))
            ),
    ]
    .align_y(Alignment::Center)
    .spacing(10)
    .into()
}

/// main content of the git sub-tab: scrollable file list + diff preview.
pub fn render_git_panel(
    collection_id: usize,
    snapshot: Option<&Result<GitStatusSnapshot, String>>,
    selected_file: Option<&PathBuf>,
    diff: Option<&(PathBuf, String)>,
) -> Element<'static, Message> {
    let snapshot = match snapshot {
        None => {
            return text("Loading git status...")
                .size(13)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
                .into();
        }
        Some(Err(e)) => {
            return column![
                text("Not a git repository yet, or git isn't available.").size(13),
                text(e.clone())
                    .size(12)
                    .color(Color::from_rgb(0.75, 0.5, 0.4)),
            ]
            .spacing(6)
            .into();
        }
        Some(Ok(s)) => s,
    };

    if snapshot.files.is_empty() {
        return text("No changes - working tree clean.")
            .size(13)
            .color(Color::from_rgb(0.5, 0.5, 0.5))
            .into();
    }

    let mut file_list = column![].spacing(2);
    for entry in &snapshot.files {
        let is_selected = selected_file == Some(&entry.path);
        file_list = file_list.push(file_row(collection_id, entry, is_selected));
    }

    let file_list_pane =
        scrollable(container(file_list).width(Length::Fill)).height(Length::FillPortion(2));

    let diff_pane: Element<Message> = match diff {
        Some((path, content)) if Some(path) == selected_file => scrollable(
            container(text(content.clone()).size(12).font(Font::MONOSPACE))
                .padding(10)
                .width(Length::Fill),
        )
        .height(Length::FillPortion(3))
        .into(),
        _ => container(
            text("Select a file to preview its diff.")
                .size(12)
                .color(Color::from_rgb(0.5, 0.5, 0.5)),
        )
        .padding(10)
        .height(Length::FillPortion(3))
        .into(),
    };

    column![file_list_pane, diff_pane]
        .spacing(10)
        .height(Length::Fill)
        .into()
}
