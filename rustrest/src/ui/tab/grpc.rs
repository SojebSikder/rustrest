use super::protocol_common::{LogEntry, grpc_log_id, message_log, simple_kv_editor};
use super::types::KeyValuePair;
use crate::message::MultilineFieldKind;
use crate::ui::multiline_input::multiline_input;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use iced::widget::{
    button, checkbox, column, container, row, scrollable, text, text_editor, text_input,
};
use iced::{Alignment, Element, Length};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GrpcTabState {
    pub endpoint: String,
    pub use_tls: bool,
    pub proto_files: Vec<PathBuf>,
    pub metadata: Vec<KeyValuePair>,
    pub target: Option<rustrest_grpc::GrpcTarget>,
    pub discovering: bool,
    pub selected_service: Option<String>,
    pub selected_method: Option<String>,
    pub request_json: text_editor::Content,
    pub log: Vec<LogEntry>,
    pub invoking: bool,
}

impl Default for GrpcTabState {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            use_tls: false,
            proto_files: Vec::new(),
            metadata: vec![KeyValuePair::new("", "")],
            target: None,
            discovering: false,
            selected_service: None,
            selected_method: None,
            request_json: text_editor::Content::with_text("{}"),
            log: Vec::new(),
            invoking: false,
        }
    }
}

impl GrpcTabState {
    pub fn selected_method_info(&self) -> Option<&rustrest_grpc::MethodInfo> {
        let target = self.target.as_ref()?;
        let service_name = self.selected_service.as_ref()?;
        let method_name = self.selected_method.as_ref()?;
        target
            .services
            .iter()
            .find(|s| &s.name == service_name)?
            .methods
            .iter()
            .find(|m| &m.name == method_name)
    }
}

#[derive(Debug, Clone)]
pub enum GrpcTabMessage {
    EndpointChanged(String),
    UseTlsToggled(bool),
    UseReflectionModeSelected,
    PickProtoFiles,
    ProtoFilesPicked(Vec<PathBuf>),
    Discover,
    ServiceSelected(String),
    MethodSelected(String),
    MetadataChanged(usize, KeyValuePair),
    AddMetadata,
    RemoveMetadata(usize),
    RequestJsonAction(text_editor::Action),
    Invoke,
    Reset,
}

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    state: &'a GrpcTabState,
    tab_id: usize,
    wrap: impl Fn(GrpcTabMessage) -> crate::message::Message + Copy + 'static,
    request_pane_height: f32,
    on_resize_start: crate::message::Message,
    multiline_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
    on_multiline_resize_start: impl Fn(MultilineFieldKind) -> crate::message::Message + Copy + 'a,
) -> Element<'a, crate::message::Message> {
    use crate::message::Message;

    let endpoint_row = row![
        text_input("host:port", &state.endpoint)
            .on_input(move |v| wrap(GrpcTabMessage::EndpointChanged(v)))
            .padding(10)
            .width(Length::Fill),
        checkbox(state.use_tls)
            .label("TLS")
            .on_toggle(move |v| wrap(GrpcTabMessage::UseTlsToggled(v))),
        button(text(if state.discovering {
            "Discovering…"
        } else {
            "Discover"
        }))
        .on_press_maybe((!state.discovering).then(|| wrap(GrpcTabMessage::Discover)))
        .padding([8, 16])
        .style(button::success),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let source_row = row![
        button(text(if state.proto_files.is_empty() {
            "Using server reflection"
        } else {
            "Using imported .proto files"
        }))
        .on_press(wrap(GrpcTabMessage::UseReflectionModeSelected))
        .padding([6, 12])
        .style(if state.proto_files.is_empty() {
            button::success
        } else {
            button::secondary
        }),
        button("Import .proto files…")
            .on_press(wrap(GrpcTabMessage::PickProtoFiles))
            .padding([6, 12]),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let services_pane: Element<'_, Message> = if let Some(target) = &state.target {
        let mut list = column![].spacing(4);
        for service in &target.services {
            list = list.push(
                button(text(service.name.clone()).size(12))
                    .on_press(wrap(GrpcTabMessage::ServiceSelected(service.name.clone())))
                    .padding(4)
                    .style(button::text)
                    .width(Length::Fill),
            );
            if state.selected_service.as_deref() == Some(service.name.as_str()) {
                for method in &service.methods {
                    let is_selected =
                        state.selected_method.as_deref() == Some(method.name.as_str());
                    list = list.push(
                        button(text(format!("  {} ({:?})", method.name, method.kind)).size(12))
                            .on_press(wrap(GrpcTabMessage::MethodSelected(method.name.clone())))
                            .padding(4)
                            .style(if is_selected {
                                button::primary
                            } else {
                                button::text
                            })
                            .width(Length::Fill),
                    );
                }
            }
        }
        scrollable(list).height(Length::Fixed(140.0)).into()
    } else {
        text("Discover a target to see its services.")
            .size(12)
            .into()
    };

    let metadata_pane = column![
        text("Metadata").size(13),
        simple_kv_editor(
            &state.metadata,
            move |i, kv| wrap(GrpcTabMessage::MetadataChanged(i, kv)),
            wrap(GrpcTabMessage::AddMetadata),
            move |i| wrap(GrpcTabMessage::RemoveMetadata(i)),
        ),
    ]
    .spacing(6);

    let request_json_editor = multiline_input(
        "{}",
        &state.request_json,
        8,
        multiline_height(MultilineFieldKind::GrpcRequestJson(tab_id)),
        move |action| wrap(GrpcTabMessage::RequestJsonAction(action)),
        Message::None,
        on_multiline_resize_start(MultilineFieldKind::GrpcRequestJson(tab_id)),
    );

    let request_editor = column![
        text("Request (JSON)").size(13),
        container(request_json_editor).style(container::bordered_box),
        button(text(if state.invoking {
            "Invoking…"
        } else {
            "Invoke"
        }))
        .on_press_maybe(
            (!state.invoking && state.selected_method_info().is_some())
                .then(|| wrap(GrpcTabMessage::Invoke))
        )
        .padding([8, 16])
        .style(button::primary),
    ]
    .spacing(6);

    let configuration_pane = column![
        endpoint_row,
        source_row,
        services_pane,
        row![metadata_pane, request_editor].spacing(12),
    ]
    .spacing(12)
    .height(Length::Fill);

    column![
        container(configuration_pane).height(Length::Fixed(request_pane_height)),
        resize_handle(DividerOrientation::Horizontal, on_resize_start),
        container(message_log(&state.log, grpc_log_id(tab_id)))
            .height(Length::Fill)
            .width(Length::Fill)
            .padding(10)
            .style(container::bordered_box),
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}
