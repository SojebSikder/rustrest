use crate::collection::git_ops::{GitChangeKind, GitFileEntry, GitRemoteOp, GitStatusSnapshot};
use crate::message::Message;
use crate::ui::spinner::spinner_with_label;
use crate::ui::tokens::RADIUS_SM;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Color, Element, Font, Length};
use std::path::PathBuf;

/// small colored letter badge for a file's git status, mirroring the log
/// level badges in `console_panel.rs`. Shared with the commit modal's file list.
pub fn status_badge(status: GitChangeKind) -> Element<'static, Message> {
    let colors = crate::theme::colors();
    let (label, color) = match status {
        GitChangeKind::Untracked => ("U", colors.ignored),
        GitChangeKind::Modified => ("M", colors.modified),
        GitChangeKind::Added => ("A", colors.created),
        GitChangeKind::Deleted => ("D", colors.deleted),
        GitChangeKind::Renamed => ("R", colors.renamed),
        GitChangeKind::Conflicted => ("!", colors.conflict),
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
            radius: RADIUS_SM.into(),
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

    button(container(content).padding([6, 8]).width(Length::Fill))
        .padding(0)
        .width(Length::Fill)
        .style(move |theme: &iced::Theme, status| {
            let colors = crate::theme::colors();
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: if is_selected {
                    Some(colors.element_selected.into())
                } else if hovered {
                    Some(colors.element_hover.into())
                } else {
                    None
                },
                border: iced::Border {
                    radius: RADIUS_SM.into(),
                    ..Default::default()
                },
                ..button::text(theme, status)
            }
        })
        .on_press(Message::GitDiffRequested(collection_id, entry_path))
        .into()
}

/// header strip for the git sub-tab: branch name + refresh/fetch/pull/push/commit buttons.
pub fn render_git_bar(
    collection_id: usize,
    snapshot: Option<&Result<GitStatusSnapshot, String>>,
    remote_op_running: Option<GitRemoteOp>,
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
    let busy = remote_op_running.is_some();
    let git_actions_label = match remote_op_running {
        Some(op) => format!("{}...", op.label()),
        None => "Git Actions \u{25BE}".to_string(),
    };

    let commit_label = if change_count > 0 {
        format!("Commit ({change_count})...")
    } else {
        "Commit...".to_string()
    };

    row![
        row![
            text("⎇").size(14),
            text(branch_label).size(13).font(Font {
                weight: iced::font::Weight::Bold,
                ..Font::DEFAULT
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill),
        button(text(git_actions_label).size(12))
            .style(button::text)
            .padding([4, 8])
            .on_press(Message::ShowGitActionsMenu(collection_id)),
        button(text("Refresh").size(12))
            .style(button::text)
            .padding([4, 8])
            .on_press(Message::GitStatusRequested(collection_id)),
        button(text(commit_label).size(12))
            .style(button::primary)
            .padding([4, 10])
            .on_press_maybe(
                (change_count > 0 && !busy).then_some(Message::CommitChangesPressed(collection_id))
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
    spinner_tick: u64,
) -> Element<'static, Message> {
    let snapshot = match snapshot {
        None => {
            return spinner_with_label(spinner_tick, "Loading git status...");
        }
        Some(Err(e)) => {
            return column![
                text("Not a git repository yet, or git isn't available.").size(13),
                text(e.clone()).size(12).color(crate::theme::colors().error),
            ]
            .spacing(6)
            .into();
        }
        Some(Ok(s)) => s,
    };

    if snapshot.files.is_empty() {
        return container(
            text("No changes \u{2014} working tree clean.")
                .size(13)
                .color(crate::theme::colors().text_muted),
        )
        .padding(12)
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
                .color(crate::theme::colors().text_muted),
        )
        .padding(12)
        .height(Length::FillPortion(3))
        .into(),
    };

    column![file_list_pane, diff_pane]
        .spacing(10)
        .height(Length::Fill)
        .into()
}
