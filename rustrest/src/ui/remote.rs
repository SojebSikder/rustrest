use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Color, Element, Font, Length};

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
}

/// the "Remote" section appended to the bottom of the sidebar: saved SSH
/// profiles (with connect/disconnect/terminal/browse actions) plus an inline
/// form to add a new one.
pub fn render_remote_section(app: &Rustrest) -> Element<'_, Message> {
    let header = text("REMOTE (SSH)")
        .size(11)
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        })
        .color(Color::from_rgb(0.55, 0.55, 0.6));

    let mut list = column![].spacing(6);
    for profile in &app.remote_profiles {
        let connected = app.remote_sessions.contains_key(&profile.id);

        let mut row_el = row![
            text(format!(
                "{}@{}:{}",
                profile.username, profile.host, profile.port
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
        } else {
            row_el = row_el.push(
                button(text("Connect").size(11))
                    .on_press(Message::RemoteConnectPressed(profile.id))
                    .padding([3, 8])
                    .style(button::primary),
            );
        }

        row_el = row_el.push(
            button(text("✕").size(11))
                .on_press(Message::RemoteDeleteProfilePressed(profile.id))
                .padding([3, 6])
                .style(button::text),
        );

        let mut item = column![row_el].spacing(4);

        if connected {
            if let Some(explorer) = app.remote_explorers.get(&profile.id) {
                if explorer.visible {
                    item = item.push(render_explorer(profile.id, explorer));
                }
            }
        }

        list = list.push(item);
    }

    let form = &app.remote_profile_form;
    let add_form = column![
        text_input("Name", &form.name)
            .on_input(Message::RemoteProfileNameChanged)
            .size(12)
            .padding(4),
        row![
            text_input("Host", &form.host)
                .on_input(Message::RemoteProfileHostChanged)
                .size(12)
                .padding(4)
                .width(Length::FillPortion(3)),
            text_input("Port", &form.port)
                .on_input(Message::RemoteProfilePortChanged)
                .size(12)
                .padding(4)
                .width(Length::FillPortion(1)),
        ]
        .spacing(4),
        text_input("Username", &form.username)
            .on_input(Message::RemoteProfileUsernameChanged)
            .size(12)
            .padding(4),
        pick_list(RemoteAuthKind::ALL, Some(form.auth_kind), |kind| {
            Message::RemoteProfileAuthKindChanged(kind)
        })
        .text_size(12),
    ]
    .spacing(4);

    let add_form = if matches!(form.auth_kind, RemoteAuthKind::PrivateKey) {
        add_form.push(
            text_input("Private key path", &form.key_path)
                .on_input(Message::RemoteProfileKeyPathChanged)
                .size(12)
                .padding(4),
        )
    } else {
        add_form
    };

    let add_form = add_form.push(
        button(text("+ Add host").size(12))
            .on_press(Message::RemoteAddProfilePressed)
            .padding([4, 8])
            .style(button::secondary),
    );

    let agent_binary_row = column![
        text("Remote agent binary (built for the remote host's OS/arch)")
            .size(10)
            .style(|_theme: &iced::Theme| text::Style {
                color: Some(Color::from_rgb(0.55, 0.55, 0.6)),
            }),
        text_input(
            "Path to rustrest-remote-agent",
            &app.remote_agent_binary_path
        )
        .on_input(Message::RemoteAgentBinaryPathChanged)
        .size(12)
        .padding(4),
    ]
    .spacing(2);

    column![
        header,
        list,
        container(text("").size(2)),
        agent_binary_row,
        add_form,
    ]
    .spacing(8)
    .into()
}

fn render_explorer<'a>(
    profile_id: usize,
    explorer: &'a RemoteExplorerState,
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
        body = body.push(text("Loading...").size(11));
    }
    if let Some(err) = &explorer.error {
        body = body.push(
            text(err.clone())
                .size(11)
                .color(Color::from_rgb(0.85, 0.35, 0.35)),
        );
    }

    let mut entries_col = column![].spacing(2);
    for entry in &explorer.entries {
        let full_path = join_remote_path(&explorer.path, &entry.name);
        let icon = if entry.is_dir { "📁" } else { "📄" };
        entries_col = entries_col.push(
            button(text(format!("{icon} {}", entry.name)).size(12))
                .on_press(Message::RemoteEntryClicked(profile_id, full_path))
                .style(button::text)
                .padding(2)
                .width(Length::Fill),
        );
    }
    body = body
        .push(scrollable(container(entries_col).width(Length::Fill)).height(Length::Fixed(160.0)));

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
