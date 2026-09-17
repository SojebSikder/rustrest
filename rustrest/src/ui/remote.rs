use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::{card, danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Font, Length, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAuthKind {
    Password,
    PrivateKey,
    Agent,
}

impl RemoteAuthKind {
    pub const ALL: [Self; 3] = [Self::Password, Self::PrivateKey, Self::Agent];

    fn label(&self) -> &'static str {
        match self {
            Self::Password => "Password",
            Self::PrivateKey => "Private key",
            Self::Agent => "SSH agent",
        }
    }
}

impl std::fmt::Display for RemoteAuthKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// inline "add SSH host" form state, shown at the bottom of the Remote
/// sidebar section.
#[derive(Debug, Clone)]
pub struct RemoteProfileForm {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_kind: RemoteAuthKind,
    pub key_path: String,
}

impl Default for RemoteProfileForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_kind: RemoteAuthKind::Password,
            key_path: String::new(),
        }
    }
}

/// a profile awaiting its password/passphrase before `RemoteConnectConfirmed`
/// actually dials out. Shown even for agent auth (with no secret field used)
/// so the connect flow doesn't need a special case.
#[derive(Debug, Clone)]
pub struct PendingRemoteConnect {
    pub profile_id: usize,
    pub secret: String,
}

/// per-connected-profile remote file browser state.
#[derive(Debug, Clone, Default)]
pub struct RemoteExplorerState {
    pub visible: bool,
    pub path: String,
    pub entries: Vec<rustrest_remote::RemoteEntry>,
    pub loading: bool,
    pub error: Option<String>,
    /// name typed into the inline "New Collection" form for the current path.
    pub new_collection_name: String,
}

pub fn view_remote_config_modal(app: &Rustrest) -> Element<'_, Message> {
    let title = text("Remote Development over SSH").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });
    let description = text(
        "Manage saved SSH hosts here. Connect, open a terminal, or browse \
         remote files.",
    )
    .size(12)
    .style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    });

    let section_header = |label: &'static str| {
        text(label).size(13).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        })
    };

    let mut saved_hosts = column![].spacing(6);
    if app.remote.remote_profiles.is_empty() {
        saved_hosts =
            saved_hosts.push(text("No saved hosts yet.").size(12).style(|theme: &Theme| {
                text::Style {
                    color: Some(muted_text_color(theme)),
                }
            }));
    }
    for profile in &app.remote.remote_profiles {
        let connected = app.remote.remote_sessions.contains_key(&profile.id);

        let mut row_el = row![
            text(format!(
                "{} ({}@{}:{})",
                profile.name, profile.username, profile.host, profile.port
            ))
            .size(12)
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        row_el = row_el.push(Space::new().width(Length::Fill));

        if connected {
            row_el = row_el
                .push(
                    button(text("Terminal").size(11))
                        .on_press(Message::RemoteOpenTerminalPressed(profile.id))
                        .padding([3, 8])
                        .style(button::secondary),
                )
                .push(
                    button(text("Browse").size(11))
                        .on_press(Message::RemoteExplorerToggled(profile.id))
                        .padding([3, 8])
                        .style(button::secondary),
                )
                .push(
                    button(text("Disconnect").size(11))
                        .on_press(Message::RemoteDisconnectPressed(profile.id))
                        .padding([3, 8])
                        .style(button::danger),
                );
        } else if app.remote.remote_connecting == Some(profile.id) {
            row_el = row_el.push(spinner_with_label(app.spinner_tick, "Connecting..."));
        } else {
            row_el = row_el.push(
                button(text("Connect").size(11))
                    .on_press(Message::RemoteConnectPressed(profile.id))
                    .padding([3, 8])
                    .style(button::primary),
            );
        }

        row_el = row_el.push(
            button(text("Delete").size(11))
                .on_press(Message::RemoteDeleteProfilePressed(profile.id))
                .padding([3, 8])
                .style(button::text),
        );

        let mut item = column![row_el].spacing(4);

        if connected {
            if let Some(explorer) = app.remote.remote_explorers.get(&profile.id) {
                if explorer.visible {
                    item = item.push(render_explorer(profile.id, explorer, app.spinner_tick));
                }
            }
        }

        saved_hosts = saved_hosts.push(item);
    }

    let form = &app.remote.remote_profile_form;
    let add_form = column![
        text_input("Name", &form.name)
            .on_input(Message::RemoteProfileNameChanged)
            .size(13)
            .padding(6),
        row![
            text_input("Host", &form.host)
                .on_input(Message::RemoteProfileHostChanged)
                .size(13)
                .padding(6)
                .width(Length::FillPortion(3)),
            text_input("Port", &form.port)
                .on_input(Message::RemoteProfilePortChanged)
                .size(13)
                .padding(6)
                .width(Length::FillPortion(1)),
        ]
        .spacing(6),
        text_input("Username", &form.username)
            .on_input(Message::RemoteProfileUsernameChanged)
            .size(13)
            .padding(6),
        pick_list(RemoteAuthKind::ALL, Some(form.auth_kind), |kind| {
            Message::RemoteProfileAuthKindChanged(kind)
        })
        .text_size(13),
    ]
    .spacing(6);

    let add_form = if matches!(form.auth_kind, RemoteAuthKind::PrivateKey) {
        add_form.push(
            text_input("Private key path", &form.key_path)
                .on_input(Message::RemoteProfileKeyPathChanged)
                .size(13)
                .padding(6),
        )
    } else {
        add_form
    };

    let add_form = add_form.push(
        button(text("+ Add host").size(13))
            .on_press(Message::RemoteAddProfilePressed)
            .padding([5, 10])
            .style(button::primary),
    );

    let agent_note = text(
        "On connect, rustrest detects the remote host's OS/architecture over \
         SSH and automatically downloads (and caches) a matching \
         rustrest-remote-agent binary from this app's GitHub releases if one \
         isn't already installed on the host.",
    )
    .size(11)
    .style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    });

    let close_btn = button(text("Close").size(14))
        .on_press(Message::CloseRemoteConfigPressed)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        description,
        section_header("Saved hosts"),
        saved_hosts,
        section_header("Add a host"),
        add_form,
        section_header("Remote agent"),
        agent_note,
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(14)
    .padding(20)
    .width(Length::Fill);

    card(
        container(scrollable(body)).height(Length::Fixed(560.0)),
        560.0,
    )
}

fn render_explorer<'a>(
    profile_id: usize,
    explorer: &'a RemoteExplorerState,
    spinner_tick: u64,
) -> Element<'a, Message> {
    let path_row = row![
        text_input("Remote path (e.g. /home/you)", &explorer.path)
            .on_input(move |txt| Message::RemoteExplorerPathChanged(profile_id, txt))
            .on_submit(Message::RemoteExplorerGoPressed(profile_id))
            .size(12)
            .padding(4)
            .width(Length::Fill),
        button(text("Go").size(11))
            .on_press(Message::RemoteExplorerGoPressed(profile_id))
            .padding([3, 8])
            .style(button::secondary),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mut body = column![path_row].spacing(4);

    if explorer.loading {
        body = body.push(spinner_with_label(spinner_tick, "Loading..."));
    }
    if let Some(err) = &explorer.error {
        body = body.push(
            text(err.clone())
                .size(11)
                .style(|theme: &Theme| text::Style {
                    color: Some(danger_text_color(theme)),
                }),
        );
    }

    let mut entries_col = column![].spacing(2);
    for entry in &explorer.entries {
        let full_path = join_remote_path(&explorer.path, &entry.name);
        let icon = if entry.is_dir { "📁" } else { "📄" };
        let mut entry_row = row![
            button(text(format!("{icon} {}", entry.name)).size(12))
                .on_press(Message::RemoteEntryClicked(profile_id, full_path.clone()))
                .style(button::text)
                .padding(2)
                .width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        if entry.is_dir {
            entry_row = entry_row.push(
                button(text("Import as collection").size(10))
                    .on_press(Message::RemoteImportDirAsCollectionPressed(
                        profile_id, full_path,
                    ))
                    .padding([2, 6])
                    .style(button::secondary),
            );
        }

        entries_col = entries_col.push(entry_row);
    }
    body = body
        .push(scrollable(container(entries_col).width(Length::Fill)).height(Length::Fixed(160.0)));

    let new_collection_row = row![
        text_input("New collection name", &explorer.new_collection_name)
            .on_input(move |txt| Message::RemoteNewCollectionNameChanged(profile_id, txt))
            .on_submit(Message::RemoteNewCollectionPressed(profile_id))
            .size(12)
            .padding(4)
            .width(Length::Fill),
        button(text("New Collection").size(11))
            .on_press(Message::RemoteNewCollectionPressed(profile_id))
            .padding([3, 8])
            .style(button::secondary),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    body = body.push(new_collection_row);

    container(body)
        .padding(6)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}

/// joins a directory path and an entry name using `/` (remote hosts targeted
/// by this feature are POSIX).
pub fn join_remote_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// modal overlay prompting for the password/passphrase (or just confirming,
/// for agent auth) needed to complete a pending connect.
pub fn view_remote_connect_modal(state: &PendingRemoteConnect) -> Element<'static, Message> {
    let title = text("Connect to remote host").size(16).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let secret_input = text_input(
        "Password / key passphrase (leave blank if none)",
        &state.secret,
    )
    .on_input(Message::RemoteConnectSecretChanged)
    .on_submit(Message::RemoteConnectConfirmed)
    .secure(true)
    .padding(6)
    .size(13);

    let cancel_btn = button(text("Cancel").size(14))
        .on_press(Message::RemoteConnectCancelled)
        .padding([8, 16])
        .style(button::secondary);
    let connect_btn = button(text("Connect").size(14))
        .on_press(Message::RemoteConnectConfirmed)
        .padding([8, 16])
        .style(button::primary);

    let footer = row![cancel_btn, connect_btn]
        .spacing(10)
        .width(Length::Fill);

    let body = column![
        title,
        secret_input,
        container(footer)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(16)
    .padding(24);

    card(body, 380.0)
}
