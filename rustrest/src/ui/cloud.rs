//! Rustrest Cloud modal: sign in, pick a team, open or upload collections, invite teammates and resolve sync conflicts.

use crate::app::Rustrest;
use crate::app::cloud::CloudModal;
use crate::message::Message;
use crate::ui::modal::{card, danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Font, Length, Theme};
use rustrest_cloud::Resolution;

/// a team as a pick-list entry
#[derive(Debug, Clone, PartialEq, Eq)]
struct TeamChoice {
    id: String,
    label: String,
}

impl std::fmt::Display for TeamChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn section_header(label: &str) -> Element<'_, Message> {
    text(label)
        .size(13)
        .font(Font {
            weight: iced::font::Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

fn muted(label: String) -> Element<'static, Message> {
    text(label)
        .size(11)
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        })
        .into()
}

pub fn view_cloud_modal<'a>(app: &'a Rustrest, modal: &'a CloudModal) -> Element<'a, Message> {
    let title = text("RustRest Cloud").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let mut body = column![title].spacing(14);

    body = match (&app.cloud.client, &app.cloud.email) {
        (Some(_), Some(email)) => signed_in_view(app, modal, email, body),
        _ => sign_in_view(modal, body),
    };

    if modal.busy {
        body = body.push(spinner_with_label(app.spinner_tick, "Working..."));
    }
    if let Some(err) = &modal.error {
        body = body.push(
            text(err.clone())
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(danger_text_color(theme)),
                }),
        );
    }

    let close_btn = button(text("Close").size(14))
        .on_press(Message::CloseCloudModal)
        .padding([8, 16])
        .style(button::secondary);
    body = body.push(
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    );

    card(
        container(scrollable(body.padding(20).width(Length::Fill))).max_height(620.0),
        560.0,
    )
}

fn sign_in_view<'a>(
    modal: &'a CloudModal,
    body: iced::widget::Column<'a, Message>,
) -> iced::widget::Column<'a, Message> {
    let mut form = column![
        muted(
            "Sync collections across devices and share them with your team. \
             Collections you upload keep working offline and sync when you save."
                .to_string()
        ),
        text_input("Server URL", &modal.server_url)
            .on_input(Message::CloudServerUrlChanged)
            .size(13)
            .padding(6),
    ]
    .spacing(8);

    if modal.sign_up {
        form = form.push(
            text_input("Your name", &modal.name)
                .on_input(Message::CloudNameChanged)
                .size(13)
                .padding(6),
        );
    }
    let submit_label = if modal.sign_up {
        "Create account"
    } else {
        "Sign in"
    };
    let submit = button(text(submit_label).size(13))
        .padding([6, 14])
        .style(button::primary)
        .on_press_maybe((!modal.busy).then_some(Message::CloudSubmitAuth));
    let toggle_label = if modal.sign_up {
        "Have an account? Sign in"
    } else {
        "New here? Create an account"
    };

    form = form
        .push(
            text_input("Email", &modal.email)
                .on_input(Message::CloudEmailChanged)
                .size(13)
                .padding(6),
        )
        .push(
            text_input("Password", &modal.password)
                .on_input(Message::CloudPasswordChanged)
                .on_submit(Message::CloudSubmitAuth)
                .secure(true)
                .size(13)
                .padding(6),
        )
        .push(
            row![
                submit,
                Space::new().width(Length::Fill),
                button(text(toggle_label).size(12))
                    .on_press(Message::CloudToggleSignUp)
                    .style(button::text),
            ]
            .align_y(Alignment::Center),
        );

    body.push(form)
}

fn signed_in_view<'a>(
    app: &'a Rustrest,
    modal: &'a CloudModal,
    email: &'a str,
    mut body: iced::widget::Column<'a, Message>,
) -> iced::widget::Column<'a, Message> {
    let server = app
        .cloud
        .client
        .as_ref()
        .map(|c| c.base_url().to_string())
        .unwrap_or_default();
    body = body.push(
        row![
            column![
                text(format!("Signed in as {email}")).size(13),
                muted(server),
            ]
            .spacing(2),
            Space::new().width(Length::Fill),
            button(text("Sync now").size(12))
                .on_press(Message::CloudSyncAll)
                .padding([4, 10])
                .style(button::secondary),
            button(text("Sign out").size(12))
                .on_press(Message::CloudSignOut)
                .padding([4, 10])
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    body = conflicts_section(app, body);

    // ---- team picker ----
    let choices: Vec<TeamChoice> = modal
        .teams
        .iter()
        .map(|t| TeamChoice {
            id: t.id.clone(),
            label: if t.is_personal {
                format!("{} (personal)", t.name)
            } else {
                t.name.clone()
            },
        })
        .collect();
    let selected = choices
        .iter()
        .find(|c| Some(&c.id) == modal.selected_team.as_ref())
        .cloned();
    let team = modal
        .teams
        .iter()
        .find(|t| Some(&t.id) == modal.selected_team.as_ref());

    body = body.push(section_header("Team")).push(
        row![
            pick_list(choices, selected, |c: TeamChoice| {
                Message::CloudTeamSelected(c.id)
            })
            .placeholder("Choose a team")
            .text_size(13)
            .width(Length::Fill),
            text_input("New team name", &modal.new_team_name)
                .on_input(Message::CloudNewTeamNameChanged)
                .on_submit(Message::CloudCreateTeam)
                .size(13)
                .padding(6)
                .width(Length::Fixed(160.0)),
            button(text("Create").size(12))
                .on_press(Message::CloudCreateTeam)
                .padding([5, 10])
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    // ---- upload ----
    if let Some(col) = modal
        .upload_collection
        .and_then(|id| app.collections.iter().find(|c| c.id == id))
    {
        let can_edit = team.is_some_and(|t| t.role.can_edit());
        body = body.push(section_header("Upload")).push(
            row![
                text(format!("Upload '{}' to this team", col.info.name))
                    .size(13)
                    .width(Length::Fill),
                button(text("Upload").size(13))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press_maybe((can_edit && !modal.busy).then_some(Message::CloudUpload)),
            ]
            .align_y(Alignment::Center),
        );
        body = body.push(muted(
            "The collection moves into RustRest's cloud cache and syncs from there; \
             the original file or folder is left as it is."
                .to_string(),
        ));
    }

    // ---- the team's collections ----
    body = body.push(section_header("Collections"));
    if modal.collections.is_empty() && !modal.busy {
        body = body.push(muted(
            "No collections yet. Right-click a collection in the sidebar and choose \
             \"Upload to Cloud...\" to add one."
                .to_string(),
        ));
    }
    let mut list = column![].spacing(4);
    for remote in &modal.collections {
        let open = app
            .cloud
            .linked
            .values()
            .any(|(_, s)| s.collection_id == remote.id);
        let action = if open {
            button(text("Open").size(12))
                .padding([3, 10])
                .style(button::secondary)
        } else {
            button(text("Open").size(12))
                .padding([3, 10])
                .style(button::primary)
                .on_press(Message::CloudOpenCollection(remote.id.clone()))
        };
        list = list.push(
            row![
                text(remote.name.clone()).size(13).width(Length::Fill),
                if open {
                    muted("already open".to_string())
                } else {
                    Space::new().into()
                },
                action,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    body = body.push(list);

    // ---- invite ----
    if let Some(team) =
        team.filter(|t| !t.is_personal && t.role == rustrest_cloud::wire::Role::Owner)
    {
        body = body.push(section_header("Invite")).push(
            row![
                text_input("Teammate's email", &modal.invite_email)
                    .on_input(Message::CloudInviteEmailChanged)
                    .on_submit(Message::CloudInvite)
                    .size(13)
                    .padding(6)
                    .width(Length::Fill),
                button(text("Add as editor").size(12))
                    .on_press(Message::CloudInvite)
                    .padding([5, 10])
                    .style(button::secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        body = body.push(muted(format!(
            "They need a RustRest Cloud account first. Editors can change every collection in {}.",
            team.name
        )));
    }

    body
}

fn conflicts_section<'a>(
    app: &'a Rustrest,
    mut body: iced::widget::Column<'a, Message>,
) -> iced::widget::Column<'a, Message> {
    let conflicted: Vec<usize> = app
        .cloud
        .linked
        .keys()
        .copied()
        .filter(|id| app.cloud.conflict_count(*id) > 0)
        .collect();
    if conflicted.is_empty() {
        return body;
    }

    body = body.push(section_header("Conflicts")).push(muted(
        "These changed both here and in the cloud. Keep yours to overwrite the cloud copy, \
         or take theirs to replace yours."
            .to_string(),
    ));
    for col_id in conflicted {
        let name = app
            .collections
            .iter()
            .find(|c| c.id == col_id)
            .map(|c| c.info.name.clone())
            .unwrap_or_default();
        let mut rows = column![text(name).size(13)].spacing(4);
        for (uid, label, deleted) in crate::app::cloud::conflict_rows(app, col_id) {
            let label = if deleted {
                format!("{label} (deleted in the cloud)")
            } else {
                label
            };
            rows = rows.push(
                row![
                    text(label).size(12).width(Length::Fill),
                    button(text("Keep mine").size(11))
                        .on_press(Message::CloudResolve(
                            col_id,
                            uid.clone(),
                            Resolution::KeepMine
                        ))
                        .padding([3, 8])
                        .style(button::secondary),
                    button(text("Take theirs").size(11))
                        .on_press(Message::CloudResolve(col_id, uid, Resolution::TakeTheirs))
                        .padding([3, 8])
                        .style(button::secondary),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        body = body.push(rows);
    }
    body
}
