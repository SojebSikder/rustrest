use crate::app::{CollectionSubTab, Rustrest};
use crate::message::{Message, MultilineFieldKind, ResizeKind};
use crate::ui::collection_settings::CollectionSettingsState;
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::docs_view::DocsState;
use crate::ui::git_panel::{render_git_bar, render_git_panel};
use crate::ui::modal::muted_text_color;
use crate::ui::script_editor::script_editor;
use crate::ui::tab::types::ScriptTab;
use crate::ui::tab::{AuthFormContext, render_auth_form};
use iced::widget::{button, checkbox, column, container, radio, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};

fn hint(label: &str) -> Element<'_, Message> {
    text(label)
        .size(12)
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        })
        .into()
}

/// Authorization sub-tab: the auth every request set to "Inherit auth from
/// parent" uses.
fn render_collection_auth<'a>(
    tab_id: usize,
    collection_id: usize,
    settings: &'a CollectionSettingsState,
    app: &'a Rustrest,
) -> Element<'a, Message> {
    let form = render_auth_form(
        &settings.auth,
        AuthFormContext {
            tab_id,
            auth_types: &rustrest_core::AuthType::PARENT,
            no_auth_note: "This collection does not use any authorization.",
            inherit_note: "",
        },
        move |msg| Message::CollectionAuth(collection_id, msg),
        // paste targets are per request tab, so these fields get Copy only
        |_, text| Message::ShowPluginTextContextMenu(text),
        move |kind: MultilineFieldKind| app.layout.multiline_height(kind),
        |kind| Message::ResizeDragStarted(ResizeKind::MultilineField(kind)),
        app.spinner_tick,
    );

    scrollable(
        column![
            hint(
                "Used by every request in this collection that is set to \"Inherit auth \
                 from parent\". A request can override it on its own Authorization tab."
            ),
            form,
        ]
        .spacing(14),
    )
    .height(Length::Fill)
    .into()
}

/// Scripts sub-tab: run before each request's own pre-request / post-response script.
fn render_collection_scripts<'a>(
    collection_id: usize,
    settings: &'a CollectionSettingsState,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut radio_bar = row![].spacing(15).align_y(Alignment::Center);
    for variant in ScriptTab::ALL {
        radio_bar = radio_bar.push(radio(
            variant.label(),
            variant,
            Some(settings.script_tab),
            move |s| Message::CollectionScriptTabChanged(collection_id, s),
        ));
    }

    let (content, note) = match settings.script_tab {
        ScriptTab::PreRequest => (
            &settings.pre_request_script,
            "Runs before every request in this collection, ahead of the request's own \
             pre-request script.",
        ),
        ScriptTab::PostResponse => (
            &settings.post_response_script,
            "Runs after every response in this collection, ahead of the request's own \
             post-response script.",
        ),
    };
    let script_tab = settings.script_tab;
    let editor = script_editor(
        content,
        theme,
        move |action| Message::CollectionScriptAction(collection_id, script_tab, action),
        Message::ShowPluginTextContextMenu(content.selection_or_text()),
    );

    column![radio_bar, hint(note), editor]
        .spacing(10)
        .height(Length::Fill)
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn render_collection_root<'a>(
    tab_id: usize,
    collection_id: usize,
    collection_name: &str,
    active_sub_tab: &CollectionSubTab,
    docs: Option<&'a DocsState>,
    settings: Option<&'a CollectionSettingsState>,
    app: &'a Rustrest,
) -> Element<'a, Message, Theme, iced::Renderer> {
    let collections = &app.collections;
    // find current live collection data
    let target_collection = collections.iter().find(|c| c.id == collection_id);
    let is_git_backed = target_collection
        .map(|c| c.storage_dir.is_some() || c.remote_dir.is_some())
        .unwrap_or(false);

    // tab headers bavigation bar
    let mut tabs_nav = row![
        button(text("Docs"))
            .style(if *active_sub_tab == CollectionSubTab::Documentation {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(
                CollectionSubTab::Documentation
            )),
        button(text("Authorization"))
            .style(if *active_sub_tab == CollectionSubTab::Authorization {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(
                CollectionSubTab::Authorization
            )),
        button(text("Variables"))
            .style(if *active_sub_tab == CollectionSubTab::Variables {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(
                CollectionSubTab::Variables
            )),
        button(text("Scripts"))
            .style(if *active_sub_tab == CollectionSubTab::Scripts {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(CollectionSubTab::Scripts)),
    ]
    .spacing(10);

    if is_git_backed {
        tabs_nav = tabs_nav.push(
            button(text("Git"))
                .style(if *active_sub_tab == CollectionSubTab::Git {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::CollectionSubTabSelected(CollectionSubTab::Git)),
        );
    }

    // content pane layout
    let content_pane: Element<'a, Message, Theme, iced::Renderer> = match active_sub_tab {
        CollectionSubTab::Authorization => match settings {
            Some(settings) => render_collection_auth(tab_id, collection_id, settings, app),
            None => column![].into(),
        },
        CollectionSubTab::Scripts => match settings {
            Some(settings) => {
                render_collection_scripts(collection_id, settings, &app.settings.theme.to_iced())
            }
            None => column![].into(),
        },
        CollectionSubTab::Variables => {
            let mut vars_column: iced::widget::Column<'_, Message, Theme, iced::Renderer> =
                column![
                    row![
                        text("").width(Length::Shrink),
                        text("Variable Key").width(Length::FillPortion(2)),
                        text("Current Value").width(Length::FillPortion(3)),
                        text("Actions").width(Length::Shrink),
                    ]
                    .spacing(10)
                    .padding(5)
                ]
                .spacing(10);

            if let Some(col) = target_collection {
                if let Some(ref variables) = col.variable {
                    for (idx, var) in variables.iter().enumerate() {
                        let key_str = var.key.clone();
                        let val_str = match &var.value {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(other) => other.to_string().trim_matches('"').to_string(),
                            None => String::new(),
                        };

                        let is_disabled = var.r#type.as_deref() == Some("disabled");

                        let val_str_for_key_input = val_str.clone();
                        let key_str_for_val_input = key_str.clone();

                        let row_item = row![
                            checkbox(!is_disabled).on_toggle(move |checked| {
                                Message::CollectionVariableToggled {
                                    collection_id,
                                    index: idx,
                                    is_active: checked,
                                }
                            }),
                            with_context_menu(
                                text_input("Variable key...", &key_str)
                                    .on_input(move |new_key| Message::CollectionVariableChanged {
                                        collection_id,
                                        index: idx,
                                        key: new_key,
                                        value: val_str_for_key_input.clone(),
                                    })
                                    .width(Length::FillPortion(2)),
                                Message::ShowTextFieldContextMenu(
                                    FieldTarget::CollectionVarKey {
                                        collection_id,
                                        index: idx,
                                    },
                                    key_str.clone(),
                                ),
                            ),
                            with_context_menu(
                                text_input("Value...", &val_str)
                                    .on_input(move |new_val| Message::CollectionVariableChanged {
                                        collection_id,
                                        index: idx,
                                        key: key_str_for_val_input.clone(),
                                        value: new_val,
                                    })
                                    .width(Length::FillPortion(3)),
                                Message::ShowTextFieldContextMenu(
                                    FieldTarget::CollectionVarValue {
                                        collection_id,
                                        index: idx,
                                    },
                                    val_str.clone(),
                                ),
                            ),
                            button(text("X")).style(button::danger).on_press(
                                Message::DeleteCollectionVariablePressed(collection_id, idx)
                            ),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center);

                        vars_column = vars_column.push(row_item);
                    }
                }
            }

            column![
                scrollable(vars_column).height(Length::FillPortion(4)),
                button(text("+ Add Variable"))
                    .on_press(Message::AddCollectionVariablePressed(collection_id))
            ]
            .spacing(15)
            .into()
        }
        CollectionSubTab::Documentation => match docs {
            Some(state) => crate::ui::docs_view::view(
                state,
                target_collection,
                &app.settings.theme.to_iced(),
                Message::Docs,
            ),
            None => column![].into(),
        },
        CollectionSubTab::Git => {
            let snapshot = app.git.git_status_cache.get(&collection_id);
            let remote_op_running = app.git.git_remote_op_running.get(&collection_id).copied();
            column![
                render_git_bar(collection_id, snapshot, remote_op_running),
                render_git_panel(
                    collection_id,
                    snapshot,
                    app.git.git_selected_file.as_ref(),
                    app.git.git_diff_cache.as_ref(),
                    app.spinner_tick,
                ),
            ]
            .spacing(12)
            .height(Length::Fill)
            .into()
        }
    };

    column![
        text(collection_name.to_string()).size(28),
        tabs_nav,
        container(content_pane)
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
    ]
    .spacing(20)
    .padding(20)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// a folder's tab
pub fn render_folder_root<'a>(
    folder_name: &str,
    docs: &'a DocsState,
    app: &'a Rustrest,
) -> Element<'a, Message, Theme, iced::Renderer> {
    let collection = app.collections.iter().find(|c| c.id == docs.collection_id);
    column![
        text(folder_name.to_string()).size(28),
        row![button(text("Docs")).style(button::primary)].spacing(10),
        container(crate::ui::docs_view::view(
            docs,
            collection,
            &app.settings.theme.to_iced(),
            Message::Docs,
        ))
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
    ]
    .spacing(20)
    .padding(20)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
