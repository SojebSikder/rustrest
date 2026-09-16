//! "Manage Plugins" tab: lists every discovered plugin, lets the user
//! enable/disable it, and surfaces load errors so a broken plugin doesn't
//! just silently vanish.

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::{danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Theme};
use rustrest_plugin_host::Capability;

/// short, user-facing label for a declared capability, shown as a badge in
/// the plugin manager so what a plugin can touch is visible before it's
/// even enabled
fn capability_label(capability: &Capability) -> &'static str {
    match capability {
        Capability::RequestHooks => "Request Hooks",
        Capability::Commands(_) => "Commands",
        Capability::MenuItems(_) => "Menu Items",
        Capability::SidebarPanel(_) => "Sidebar Panel",
        Capability::ImportFormat(_) => "Import Format",
        Capability::ExportFormat(_) => "Export Format",
        Capability::ExternalProcess => "External Process",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManagerAction {
    Installing,
    Uninstalling(String),
}

pub fn render_plugin_manager_tab(app: &Rustrest) -> Element<'_, Message> {
    let title = text("Manage Plugins").size(28);

    let is_busy = app.plugin_manager_busy.is_some();
    let install_control: Element<'_, Message> =
        if app.plugin_manager_busy == Some(PluginManagerAction::Installing) {
            spinner_with_label(app.spinner_tick, "Installing...")
        } else {
            button(text("Install Plugin Folder...").size(13))
                .on_press_maybe((!is_busy).then_some(Message::InstallPluginPressed))
                .padding([6, 12])
                .style(button::secondary)
                .into()
        };

    let header = row![
        title,
        container(install_control)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .align_y(Alignment::Center);

    let installed = app.plugin_manager.installed();
    let mut list = column![].spacing(10);

    if installed.is_empty() {
        list = list.push(
            text(format!(
                "No plugins found in {}",
                app.plugin_manager.plugins_dir().display()
            ))
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
        );
    }

    for plugin in installed {
        let id = plugin.id().to_string();
        let uninstall_control: Element<'_, Message> = if app.plugin_manager_busy
            == Some(PluginManagerAction::Uninstalling(id.clone()))
        {
            spinner_with_label(app.spinner_tick, "Uninstalling...")
        } else {
            button(text("Uninstall").size(12))
                .on_press_maybe((!is_busy).then_some(Message::UninstallPluginPressed(id.clone())))
                .padding([4, 10])
                .style(button::danger)
                .into()
        };

        let info: Element<'_, Message> = match &plugin.manifest {
            Some(manifest) => {
                let meta_row = row![
                    checkbox(plugin.enabled)
                        .label(manifest.name.clone())
                        .on_toggle(move |enabled| {
                            Message::TogglePluginEnabled(id.clone(), enabled)
                        }),
                    text(format!("v{}", manifest.version))
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                    text(format!("by {}", manifest.author))
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center);

                let mut badges = row![].spacing(6);
                for capability in &manifest.capabilities {
                    let is_external_process = matches!(capability, Capability::ExternalProcess);
                    badges = badges.push(
                        container(text(capability_label(capability)).size(10))
                            .padding([2, 6])
                            .style(move |theme: &Theme| {
                                let mut style = container::rounded_box(theme);
                                if is_external_process {
                                    style.text_color = Some(danger_text_color(theme));
                                } else {
                                    style.text_color = Some(muted_text_color(theme));
                                }
                                style
                            }),
                    );
                }

                column![
                    meta_row,
                    text(manifest.description.clone())
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                    badges,
                ]
                .spacing(4)
                .into()
            }
            None => column![
                text(format!("{} (failed to load)", plugin.dir_name))
                    .size(13)
                    .style(|theme: &Theme| text::Style {
                        color: Some(danger_text_color(theme)),
                    }),
                text(plugin.load_error.clone().unwrap_or_default())
                    .size(11)
                    .style(|theme: &Theme| text::Style {
                        color: Some(danger_text_color(theme)),
                    }),
            ]
            .spacing(4)
            .into(),
        };

        let row_content = row![container(info).width(Length::Fill), uninstall_control]
            .spacing(10)
            .align_y(Alignment::Center);

        list = list.push(
            container(row_content)
                .padding(10)
                .width(Length::Fill)
                .style(container::bordered_box),
        );
    }

    column![header, scrollable(list).height(Length::Fill)]
        .spacing(20)
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
