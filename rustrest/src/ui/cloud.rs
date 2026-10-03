//! Rustrest Cloud modal: sign in, manage teams and members, open or upload collections and resolve sync conflicts.

use crate::app::Rustrest;
use crate::app::cloud::CloudModal;
use crate::message::Message;
use crate::ui::modal::{card, danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Space, button, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Font, Length, Theme};
use rustrest_cloud::Resolution;
use rustrest_cloud::wire::{Role, Team};

const CARD_WIDTH: f32 = 580.0;
/// height of the scrolling middle, the header and footer stay put
const BODY_MAX_HEIGHT: f32 = 470.0;
const ROLE_WIDTH: f32 = 96.0;

/// a team as a pick-list entry
#[derive(Debug, Clone, PartialEq, Eq)]
struct TeamChoice {
    id: String,
    label: String,
}

impl TeamChoice {
    fn new(team: &Team) -> Self {
        Self {
            id: team.id.clone(),
            label: if team.is_personal {
                format!("{} (personal)", team.name)
            } else {
                team.name.clone()
            },
        }
    }
}

impl std::fmt::Display for TeamChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

// ---- building blocks ----

fn semibold() -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    }
}

/// text that takes the rest of its row and wraps, so long names and emails
/// never push the row's buttons out of the card
fn fill_text<'a>(label: impl text::IntoFragment<'a>, size: u32) -> text::Text<'a, Theme> {
    text(label)
        .size(size)
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}

fn muted<'a>(label: impl text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    fill_text(label, 11).style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    })
}

/// small uppercase caption above a panel, with optional actions on the right
fn section<'a>(
    title: &'a str,
    trailing: Option<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut header = row![
        text(title.to_uppercase())
            .size(11)
            .font(semibold())
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(8);
    if let Some(trailing) = trailing {
        header = header.push(trailing);
    }
    column![header, panel(content)].spacing(6).into()
}

fn panel<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(12)
        .width(Length::Fill)
        .style(|_: &Theme| {
            let colors = crate::theme::colors();
            container::Style {
                background: Some(colors.surface_background.into()),
                border: Border {
                    color: colors.border_variant,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

/// tinted box for confirmations and warnings
fn callout<'a>(content: impl Into<Element<'a, Message>>, danger: bool) -> Element<'a, Message> {
    container(content)
        .padding(10)
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            let tint = if danger {
                danger_text_color(theme)
            } else {
                crate::theme::colors().info
            };
            container::Style {
                background: Some(Color { a: 0.08, ..tint }.into()),
                border: Border {
                    color: Color { a: 0.4, ..tint },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn badge<'a>(label: impl text::IntoFragment<'a>, color: Color) -> Element<'a, Message> {
    container(text(label).size(10).font(semibold()).color(color))
        .padding([1, 6])
        .style(move |_: &Theme| container::Style {
            background: Some(Color { a: 0.14, ..color }.into()),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn role_badge<'a>(role: Role) -> Element<'a, Message> {
    let colors = crate::theme::colors();
    let color = match role {
        Role::Owner => colors.text_accent,
        Role::Editor => colors.success,
        Role::Viewer => colors.text_muted,
    };
    badge(role.to_string(), color)
}

/// round badge with the first letter of a member's name
fn avatar<'a>(label: &str) -> Element<'a, Message> {
    let initial = label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    container(text(initial).size(12).font(semibold()))
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .center(Length::Fixed(28.0))
        .style(|_: &Theme| {
            let colors = crate::theme::colors();
            container::Style {
                background: Some(colors.element_selected.into()),
                text_color: Some(colors.text),
                border: Border {
                    radius: 14.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

fn small_button<'a>(label: &'a str) -> button::Button<'a, Message> {
    button(text(label).size(12)).padding([4, 10])
}

fn divider<'a>() -> Element<'a, Message> {
    rule::horizontal(1).style(rule::weak).into()
}

/// stacks rows with a hairline between each
fn divided<'a>(rows: Vec<Element<'a, Message>>) -> Column<'a, Message> {
    let mut list = column![].spacing(8);
    for (i, entry) in rows.into_iter().enumerate() {
        if i > 0 {
            list = list.push(divider());
        }
        list = list.push(entry);
    }
    list
}

// ---- modal ----

pub fn view_cloud_modal<'a>(app: &'a Rustrest, modal: &'a CloudModal) -> Element<'a, Message> {
    let signed_in = match (&app.cloud.client, &app.cloud.email) {
        (Some(client), Some(email)) => Some((client.base_url().to_string(), email.as_str())),
        _ => None,
    };

    let title = text("Rustrest Cloud").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });
    let header: Element<'a, Message> = match &signed_in {
        Some((server, email)) => row![
            column![
                title,
                muted(format!("Signed in as {email} \u{00b7} {server}")),
            ]
            .spacing(2)
            .width(Length::Fill),
            small_button("Sync now")
                .on_press(Message::CloudSyncAll)
                .style(button::secondary),
            small_button("Sign out")
                .on_press(Message::CloudSignOut)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        None => title.into(),
    };

    let sections = match signed_in {
        Some(_) => signed_in_view(app, modal),
        None => sign_in_view(modal),
    };
    let body = scrollable(sections.padding(iced::Padding {
        right: 4.0,
        ..iced::Padding::ZERO
    }))
    .direction(scrollable::Direction::Vertical(
        scrollable::Scrollbar::new()
            .width(6)
            .scroller_width(6)
            .spacing(6),
    ));

    // ---- footer: status on the left, close on the right ----
    let status: Element<'a, Message> = if let Some(err) = &modal.error {
        fill_text(err.as_str(), 12)
            .style(|theme: &Theme| text::Style {
                color: Some(danger_text_color(theme)),
            })
            .into()
    } else if modal.busy {
        container(spinner_with_label(app.spinner_tick, "Working..."))
            .width(Length::Fill)
            .into()
    } else {
        Space::new().width(Length::Fill).into()
    };
    let footer = row![
        status,
        button(text("Close").size(13))
            .on_press(Message::CloseCloudModal)
            .padding([6, 16])
            .style(button::secondary),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    card(
        column![
            header,
            divider(),
            container(body).max_height(BODY_MAX_HEIGHT),
            divider(),
            footer,
        ]
        .spacing(14)
        .padding(20)
        .width(Length::Fill),
        CARD_WIDTH,
    )
}

fn sign_in_view<'a>(modal: &'a CloudModal) -> Column<'a, Message> {
    let input = |placeholder: &str, value: &'a str| {
        text_input(placeholder, value).size(13).padding([7, 10])
    };

    let mut form =
        column![input("Server URL", &modal.server_url).on_input(Message::CloudServerUrlChanged)]
            .spacing(8);
    if modal.sign_up {
        form = form.push(input("Your name", &modal.name).on_input(Message::CloudNameChanged));
    }
    form = form
        .push(input("Email", &modal.email).on_input(Message::CloudEmailChanged))
        .push(
            input("Password", &modal.password)
                .on_input(Message::CloudPasswordChanged)
                .on_submit(Message::CloudSubmitAuth)
                .secure(true),
        );

    let (submit_label, toggle_label) = if modal.sign_up {
        ("Create account", "Have an account? Sign in")
    } else {
        ("Sign in", "New here? Create an account")
    };
    form = form.push(
        row![
            button(text(submit_label).size(13))
                .padding([6, 16])
                .style(button::primary)
                .on_press_maybe((!modal.busy).then_some(Message::CloudSubmitAuth)),
            Space::new().width(Length::Fill),
            button(text(toggle_label).size(12))
                .on_press(Message::CloudToggleSignUp)
                .style(button::text),
        ]
        .align_y(Alignment::Center),
    );

    column![
        muted(
            "Sync collections across devices and share them with your team. \
             Collections you upload keep working offline and sync when you save."
        ),
        panel(form),
    ]
    .spacing(14)
}

fn signed_in_view<'a>(app: &'a Rustrest, modal: &'a CloudModal) -> Column<'a, Message> {
    let team = modal
        .teams
        .iter()
        .find(|t| Some(&t.id) == modal.selected_team.as_ref());

    let mut body = column![].spacing(18);
    if let Some(conflicts) = conflicts_section(app) {
        body = body.push(conflicts);
    }
    body = body.push(team_section(modal, team));
    if let Some(upload) = upload_section(app, modal, team) {
        body = body.push(upload);
    }
    body = body.push(collections_section(app, modal, team));
    if let Some(team) = team.filter(|t| !t.is_personal) {
        body = body.push(members_section(app, modal, team));
    }
    body
}

fn team_section<'a>(modal: &'a CloudModal, team: Option<&'a Team>) -> Element<'a, Message> {
    let choices: Vec<TeamChoice> = modal.teams.iter().map(TeamChoice::new).collect();
    let selected = team.map(TeamChoice::new);
    let is_owner = team.is_some_and(|t| t.role == Role::Owner);

    let mut content = column![].spacing(10);

    match &modal.rename_team {
        Some(name) => {
            content = content.push(
                row![
                    text_input("Team name", name)
                        .on_input(Message::CloudRenameTeamChanged)
                        .on_submit(Message::CloudSubmitRenameTeam)
                        .size(13)
                        .padding([6, 10])
                        .width(Length::Fill),
                    small_button("Save")
                        .on_press_maybe(
                            (!modal.busy && !name.trim().is_empty())
                                .then_some(Message::CloudSubmitRenameTeam),
                        )
                        .style(button::primary),
                    small_button("Cancel")
                        .on_press(Message::CloudCancelRenameTeam)
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        None => {
            let mut picker = row![
                pick_list(choices, selected, |c: TeamChoice| {
                    Message::CloudTeamSelected(c.id)
                })
                .placeholder("Choose a team")
                .text_size(13)
                .padding([6, 10])
                .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if is_owner {
                picker = picker.push(
                    small_button("Rename")
                        .on_press_maybe((!modal.busy).then_some(Message::CloudStartRenameTeam))
                        .style(button::secondary),
                );
            }
            if is_owner && team.is_some_and(|t| !t.is_personal) {
                picker = picker.push(
                    small_button("Delete")
                        .on_press_maybe(
                            (!modal.busy && !modal.confirm_delete_team)
                                .then_some(Message::CloudDeleteTeam),
                        )
                        .style(button::danger),
                );
            }
            content = content.push(picker);
        }
    }

    if let Some(team) = team.filter(|_| modal.confirm_delete_team) {
        let count = modal.collections.len();
        let collections = match count {
            1 => "its collection".to_string(),
            n => format!("its {n} collections"),
        };
        content = content.push(callout(
            column![
                fill_text(format!("Delete '{}'?", team.name), 13).font(semibold()),
                muted(format!(
                    "This removes {collections} and environments for every member and can't be \
                     undone. Copies already open on someone's machine are kept as local collections."
                )),
                row![
                    Space::new().width(Length::Fill),
                    small_button("Cancel")
                        .on_press(Message::CloudCancelDeleteTeam)
                        .style(button::secondary),
                    small_button("Delete team")
                        .on_press_maybe((!modal.busy).then_some(Message::CloudDeleteTeam))
                        .style(button::danger),
                ]
                .spacing(8),
            ]
            .spacing(6),
            true,
        ));
    }

    if team.is_some_and(|t| t.is_personal) {
        content = content.push(muted(
            "Your personal workspace is private. Create a team to share collections.",
        ));
    }

    content = content.push(divider()).push(
        row![
            text_input("New team name", &modal.new_team_name)
                .on_input(Message::CloudNewTeamNameChanged)
                .on_submit(Message::CloudCreateTeam)
                .size(13)
                .padding([6, 10])
                .width(Length::Fill),
            small_button("Create team")
                .on_press_maybe(
                    (!modal.busy && !modal.new_team_name.trim().is_empty())
                        .then_some(Message::CloudCreateTeam),
                )
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    section("Team", team.map(|t| role_badge(t.role)), content)
}

fn upload_section<'a>(
    app: &'a Rustrest,
    modal: &'a CloudModal,
    team: Option<&'a Team>,
) -> Option<Element<'a, Message>> {
    let col = modal
        .upload_collection
        .and_then(|id| app.collections.iter().find(|c| c.id == id))?;
    let can_edit = team.is_some_and(|t| t.role.can_edit());

    let mut content = column![
        row![
            fill_text(format!("Upload '{}' to this team", col.info.name), 13),
            button(text("Upload").size(13))
                .padding([6, 16])
                .style(button::primary)
                .on_press_maybe((can_edit && !modal.busy).then_some(Message::CloudUpload)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        muted(
            "The collection moves into Rustrest's cloud cache and syncs from there; \
             the original file or folder is left as it is."
        ),
    ]
    .spacing(6);
    if !can_edit && team.is_some() {
        content = content.push(muted("You need the editor role in this team to upload."));
    }
    Some(section("Upload", None, content))
}

fn collections_section<'a>(
    app: &'a Rustrest,
    modal: &'a CloudModal,
    team: Option<&'a Team>,
) -> Element<'a, Message> {
    // only an owner can take a collection out of this team, and only into teams they can edit
    let is_owner = team.is_some_and(|t| t.role == Role::Owner);
    let move_choices: Vec<TeamChoice> = if is_owner {
        modal
            .teams
            .iter()
            .filter(|t| Some(&t.id) != modal.selected_team.as_ref() && t.role.can_edit())
            .map(TeamChoice::new)
            .collect()
    } else {
        Vec::new()
    };

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for remote in &modal.collections {
        let open = app
            .cloud
            .linked
            .values()
            .any(|(_, s)| s.collection_id == remote.id);

        let mut entry = row![fill_text(remote.name.as_str(), 13)]
            .spacing(8)
            .align_y(Alignment::Center);
        if !move_choices.is_empty() {
            let collection_id = remote.id.clone();
            entry = entry.push(
                pick_list(move_choices.clone(), None::<TeamChoice>, move |c| {
                    Message::CloudMoveTargetPicked(collection_id.clone(), c.id)
                })
                .placeholder("Move to...")
                .text_size(12)
                .padding([4, 8])
                .width(Length::Fixed(130.0)),
            );
        }
        entry = entry.push(if open {
            badge("Open", crate::theme::colors().success)
        } else {
            small_button("Open")
                .on_press_maybe(
                    (!modal.busy).then(|| Message::CloudOpenCollection(remote.id.clone())),
                )
                .style(button::primary)
                .into()
        });

        let mut item = column![entry].spacing(8);
        // confirmation for a picked move target
        if let Some((_, target)) = modal
            .move_target
            .as_ref()
            .filter(|(collection_id, _)| *collection_id == remote.id)
        {
            let target_name = move_choices
                .iter()
                .find(|c| &c.id == target)
                .map(|c| c.label.clone())
                .unwrap_or_default();
            item = item.push(callout(
                column![
                    fill_text(format!("Move '{}' to {target_name}?", remote.name), 12)
                        .font(semibold()),
                    muted(format!(
                        "Members of {} who aren't in {target_name} lose access to it.",
                        team.map(|t| t.name.as_str()).unwrap_or("this team"),
                    )),
                    row![
                        Space::new().width(Length::Fill),
                        small_button("Cancel")
                            .on_press(Message::CloudCancelMove)
                            .style(button::secondary),
                        small_button("Move")
                            .on_press_maybe((!modal.busy).then_some(Message::CloudConfirmMove))
                            .style(button::primary),
                    ]
                    .spacing(8),
                ]
                .spacing(6),
                false,
            ));
        }
        rows.push(item.into());
    }

    let content: Element<'a, Message> = if rows.is_empty() {
        muted(if modal.busy {
            "Loading..."
        } else {
            "No collections yet. Right-click a collection in the sidebar and choose \
             \"Upload to Cloud...\" to add one."
        })
        .into()
    } else {
        divided(rows).into()
    };
    let count = (!modal.collections.is_empty()).then(|| {
        badge(
            modal.collections.len().to_string(),
            crate::theme::colors().text_muted,
        )
    });
    section("Collections", count, content)
}

fn members_section<'a>(
    app: &'a Rustrest,
    modal: &'a CloudModal,
    team: &'a Team,
) -> Element<'a, Message> {
    let is_owner = team.role == Role::Owner;
    let me = app.cloud.email.as_deref().unwrap_or_default();

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for member in &modal.members {
        let is_me = member
            .email
            .as_deref()
            .is_some_and(|e| e.eq_ignore_ascii_case(me));
        let name = crate::app::cloud::member_label(member);
        let confirming = modal.confirm_remove_member.as_ref() == Some(&member.user_id);

        let mut details = column![fill_text(
            if is_me {
                format!("{name} (you)")
            } else {
                name.clone()
            },
            13
        )]
        .spacing(1)
        .width(Length::Fill);
        if let Some(email) = member.email.as_deref().filter(|e| *e != name) {
            details = details.push(muted(email));
        }

        let role: Element<'a, Message> = if is_owner && !confirming {
            let user_id = member.user_id.clone();
            pick_list(&Role::ALL[..], Some(member.role), move |role| {
                Message::CloudSetMemberRole(user_id.clone(), role)
            })
            .text_size(12)
            .padding([4, 8])
            .width(Length::Fixed(ROLE_WIDTH))
            .into()
        } else {
            role_badge(member.role)
        };

        let mut entry = row![avatar(&name), details, role]
            .spacing(10)
            .align_y(Alignment::Center);
        // owners can remove anyone, everyone can leave
        if (is_owner || is_me) && !confirming {
            entry = entry.push(
                small_button(if is_me { "Leave" } else { "Remove" })
                    .on_press_maybe(
                        (!modal.busy).then(|| Message::CloudRemoveMember(member.user_id.clone())),
                    )
                    .style(button::secondary),
            );
        }

        let mut item = column![entry].spacing(8);
        if confirming {
            let (question, action) = if is_me {
                (format!("Leave {}?", team.name), "Leave team")
            } else {
                (format!("Remove {name} from {}?", team.name), "Remove")
            };
            item = item.push(callout(
                column![
                    fill_text(question, 12).font(semibold()),
                    muted("They lose access to this team's collections and environments."),
                    row![
                        Space::new().width(Length::Fill),
                        small_button("Cancel")
                            .on_press(Message::CloudCancelRemoveMember)
                            .style(button::secondary),
                        small_button(action)
                            .on_press_maybe(
                                (!modal.busy)
                                    .then(|| Message::CloudRemoveMember(member.user_id.clone())),
                            )
                            .style(button::danger),
                    ]
                    .spacing(8),
                ]
                .spacing(6),
                true,
            ));
        }
        rows.push(item.into());
    }

    let mut content = column![].spacing(12);
    content = content.push(if rows.is_empty() {
        Element::from(muted(if modal.busy {
            "Loading..."
        } else {
            "No members"
        }))
    } else {
        divided(rows).into()
    });

    if is_owner {
        content = content.push(divider()).push(
            column![
                row![
                    text_input("Teammate's email", &modal.invite_email)
                        .on_input(Message::CloudInviteEmailChanged)
                        .on_submit(Message::CloudInvite)
                        .size(13)
                        .padding([6, 10])
                        .width(Length::Fill),
                    pick_list(
                        &Role::ALL[..],
                        Some(modal.invite_role),
                        Message::CloudInviteRoleChanged
                    )
                    .text_size(12)
                    .padding([6, 8])
                    .width(Length::Fixed(ROLE_WIDTH)),
                    small_button("Invite")
                        .on_press_maybe(
                            (!modal.busy && !modal.invite_email.trim().is_empty())
                                .then_some(Message::CloudInvite),
                        )
                        .style(button::primary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                muted(
                    "They need a Rustrest Cloud account first. Viewers can open collections, \
                     editors can change them, owners can also manage the team."
                ),
            ]
            .spacing(6),
        );
    }

    let count = (!modal.members.is_empty()).then(|| {
        badge(
            modal.members.len().to_string(),
            crate::theme::colors().text_muted,
        )
    });
    section("Members", count, content)
}

fn conflicts_section<'a>(app: &'a Rustrest) -> Option<Element<'a, Message>> {
    let conflicted: Vec<usize> = app
        .cloud
        .linked
        .keys()
        .copied()
        .filter(|id| app.cloud.conflict_count(*id) > 0)
        .collect();
    if conflicted.is_empty() {
        return None;
    }

    let mut content = column![muted(
        "These changed both here and in the cloud. Keep yours to overwrite the cloud copy, \
         or take theirs to replace yours."
    )]
    .spacing(10);
    for col_id in conflicted {
        let name = app
            .collections
            .iter()
            .find(|c| c.id == col_id)
            .map(|c| c.info.name.clone())
            .unwrap_or_default();
        let mut rows: Vec<Element<'a, Message>> = Vec::new();
        for (uid, label, deleted) in crate::app::cloud::conflict_rows(app, col_id) {
            let label = if deleted {
                format!("{label} (deleted in the cloud)")
            } else {
                label
            };
            rows.push(
                row![
                    fill_text(label, 12),
                    small_button("Keep mine")
                        .on_press(Message::CloudResolve(
                            col_id,
                            uid.clone(),
                            Resolution::KeepMine
                        ))
                        .style(button::secondary),
                    small_button("Take theirs")
                        .on_press(Message::CloudResolve(col_id, uid, Resolution::TakeTheirs))
                        .style(button::secondary),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        content =
            content.push(column![fill_text(name, 13).font(semibold()), divided(rows)].spacing(6));
    }
    Some(section(
        "Conflicts",
        Some(badge("Needs attention", crate::theme::colors().warning)),
        content,
    ))
}
