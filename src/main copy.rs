// #![windows_subsystem = "windows"]

// mod collection;
// mod env;
// mod http_client;
// mod tab;

// use collection::{CollectionItem, PostmanCollection, PostmanRequestNode, create_tab_from_request};
// use env::Environment;
// use http_client::{HttpMethod, HttpResponse, send_request};
// use tab::{Tab, TabMessage};
// use tokio_util::sync::CancellationToken;

// use iced::widget::{
//     button, column, container, pick_list, row, scrollable, text, text_editor, text_input,
// };
// use iced::{Alignment, Element, Font, Length, Padding, Size, Task};

// const APP_NAME: &str = "Rustrest";
// const APP_VERSION: &str = "0.1.0";

// pub fn main() -> iced::Result {
//     iced::application(init, update, view)
//         .title(|_: &Rustrest| format!("{} - API Testing Platform", APP_NAME))
//         .window(iced::window::Settings {
//             size: Size::new(1250.0, 850.0),
//             ..Default::default()
//         })
//         .run()
// }

// /// Defines what context is loaded inside an operational UI tab workspace
// #[derive(Debug, Clone)]
// enum WorkspaceContent {
//     HttpRequest,
//     CollectionRoot {
//         collection_id: usize,
//         collection_name: String,
//     },
// }

// struct Rustrest {
//     collections: Vec<PostmanCollection>,
//     environments: Vec<Environment>,
//     active_env_index: Option<usize>,
//     tabs: Vec<TabState>,
//     active_tab_index: usize,
//     next_tab_id: usize,
// }

// struct TabState {
//     tab: Tab,
//     content: WorkspaceContent,
//     is_editing_name: bool,
// }

// #[derive(Debug, Clone)]
// enum Message {
//     TabSelected(usize),
//     NewTabPressed,
//     SidebarCollectionRootClicked(usize),
//     CloseTabPressed(usize),
//     ActiveTabMessage(TabMessage),
//     SendPressed,
//     ResponseReceived(usize, Result<HttpResponse, String>),
//     TabNameDoubleClick(usize),
//     TabNameChanged(usize, String),
//     TabNameSave(usize),
//     ImportCollectionPressed,
//     SidebarRequestClicked(PostmanRequestNode),

//     // Environment Actions
//     EnvSelected(Option<String>),
// }

// fn init() -> (Rustrest, Task<Message>) {
//     let mut demo_env = Environment::new("Default");
//     if !demo_env.variables.is_empty() {
//         demo_env.variables[0].is_active = true;
//     }

//     (
//         Rustrest {
//             collections: Vec::new(),
//             environments: vec![demo_env],
//             active_env_index: None,
//             tabs: vec![TabState {
//                 tab: Tab::new(1),
//                 content: WorkspaceContent::HttpRequest,
//                 is_editing_name: false,
//             }],
//             active_tab_index: 0,
//             next_tab_id: 2,
//         },
//         Task::none(),
//     )
// }

// fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
//     match message {
//         Message::ImportCollectionPressed => {
//             if let Some(path) = rfd::FileDialog::new()
//                 .add_filter("Postman Collection", &["json"])
//                 .pick_file()
//             {
//                 if let Ok(file_content) = std::fs::read_to_string(path) {
//                     if let Ok(mut collection) =
//                         serde_json::from_str::<PostmanCollection>(&file_content)
//                     {
//                         collection.id = app.next_tab_id;
//                         app.next_tab_id += 1;
//                         app.collections.push(collection);
//                     }
//                 }
//             }
//             Task::none()
//         }

//         Message::SidebarCollectionRootClicked(col_id) => {
//             // check if workspace tab is already open for this collection root node
//             let existing_tab_idx = app.tabs.iter().position(|t| {
//                 if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
//                     collection_id == col_id
//                 } else {
//                     false
//                 }
//             });

//             if let Some(idx) = existing_tab_idx {
//                 app.active_tab_index = idx;
//             } else if let Some(col) = app.collections.iter().find(|c| c.id == col_id) {
//                 let mut root_tab = Tab::new(app.next_tab_id);
//                 root_tab.name = col.info.name.clone();

//                 app.tabs.push(TabState {
//                     tab: root_tab,
//                     content: WorkspaceContent::CollectionRoot {
//                         collection_id: col_id,
//                         collection_name: col.info.name.clone(),
//                     },
//                     is_editing_name: false,
//                 });
//                 app.next_tab_id += 1;
//                 app.active_tab_index = app.tabs.len() - 1;
//             }
//             Task::none()
//         }

//         Message::SidebarRequestClicked(req_node) => {
//             let associated_collection_id = app
//                 .collections
//                 .iter()
//                 .find(|c| contains_request_node(&c.item, &req_node.name))
//                 .map(|c| c.id);

//             let new_tab =
//                 create_tab_from_request(app.next_tab_id, &req_node, associated_collection_id);

//             app.tabs.push(TabState {
//                 tab: new_tab,
//                 content: WorkspaceContent::HttpRequest,
//                 is_editing_name: false,
//             });
//             app.next_tab_id += 1;
//             app.active_tab_index = app.tabs.len() - 1;
//             Task::none()
//         }

//         Message::TabSelected(index) => {
//             if index < app.tabs.len() {
//                 app.active_tab_index = index;
//             }
//             Task::none()
//         }

//         Message::NewTabPressed => {
//             app.tabs.push(TabState {
//                 tab: Tab::new(app.next_tab_id),
//                 content: WorkspaceContent::HttpRequest,
//                 is_editing_name: false,
//             });
//             app.active_tab_index = app.tabs.len() - 1;
//             app.next_tab_id += 1;
//             Task::none()
//         }

//         Message::CloseTabPressed(index) => {
//             if app.tabs.len() > 1 {
//                 if let Some(tab_state) = app.tabs.get(index) {
//                     if tab_state.tab.is_loading {
//                         tab_state.tab.cancel_token.cancel();
//                     }
//                 }
//                 app.tabs.remove(index);
//                 if app.active_tab_index >= app.tabs.len() {
//                     app.active_tab_index = app.tabs.len() - 1;
//                 }
//             }
//             Task::none()
//         }

//         Message::ActiveTabMessage(tab_msg) => {
//             if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
//                 if let TabMessage::ResponseViewChanged(view) = tab_msg {
//                     tab_state.tab.response_view = view;
//                     if let Some(Ok(resp)) = &tab_state.tab.response {
//                         let body_text = match view {
//                             tab::types::ResponseView::Json => format_json_or_fallback(&resp.body),
//                             tab::types::ResponseView::Raw => resp.body.clone(),
//                         };
//                         tab_state.tab.response_body_editor =
//                             text_editor::Content::with_text(&body_text);
//                     }
//                 } else {
//                     tab_state.tab.update(tab_msg);
//                 }
//             }
//             Task::none()
//         }

//         Message::SendPressed => {
//             if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
//                 // safeguard against triggering network requests if looking at a collection overview
//                 if let WorkspaceContent::CollectionRoot { .. } = tab_state.content {
//                     return Task::none();
//                 }

//                 let tab = &mut tab_state.tab;
//                 if tab.is_loading || tab.url.is_empty() {
//                     return Task::none();
//                 }

//                 let tab_id = tab.id;
//                 tab.cancel_token = CancellationToken::new();
//                 tab.is_loading = true;
//                 tab.response = None;

//                 let active_env = app
//                     .active_env_index
//                     .and_then(|idx| app.environments.get(idx))
//                     .cloned();

//                 let collection_vars = tab
//                     .collection_id
//                     .and_then(|c_id| app.collections.iter().find(|c| c.id == c_id))
//                     .map(|c| c.get_native_variables());

//                 let (
//                     final_url,
//                     compiled_body,
//                     compiled_form_data,
//                     filtered_headers,
//                     filtered_cookies,
//                     compiled_auth,
//                 ) = tab.compile_request_fields(&active_env, collection_vars.as_deref());

//                 return Task::perform(
//                     send_request(
//                         final_url,
//                         tab.method.clone(),
//                         tab.body_type,
//                         compiled_body,
//                         compiled_form_data,
//                         tab.binary_file_path.clone(),
//                         filtered_headers,
//                         filtered_cookies,
//                         compiled_auth,
//                         tab.cancel_token.clone(),
//                     ),
//                     move |res| Message::ResponseReceived(tab_id, res),
//                 );
//             }
//             Task::none()
//         }

//         Message::ResponseReceived(tab_id, res) => {
//             if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
//                 let tab = &mut tab_state.tab;
//                 tab.is_loading = false;
//                 match &res {
//                     Ok(resp) => {
//                         let initial_body = match tab.response_view {
//                             tab::types::ResponseView::Json => format_json_or_fallback(&resp.body),
//                             tab::types::ResponseView::Raw => resp.body.clone(),
//                         };
//                         tab.response_body_editor = text_editor::Content::with_text(&initial_body);
//                     }
//                     Err(err_msg) => {
//                         tab.response_body_editor = text_editor::Content::with_text(err_msg);
//                     }
//                 }
//                 tab.response = Some(res);
//             }
//             Task::none()
//         }

//         Message::TabNameDoubleClick(idx) => {
//             if let Some(tab_state) = app.tabs.get_mut(idx) {
//                 tab_state.is_editing_name = true;
//             }
//             Task::none()
//         }

//         Message::TabNameChanged(idx, new_name) => {
//             if let Some(tab_state) = app.tabs.get_mut(idx) {
//                 tab_state.tab.name = new_name;
//             }
//             Task::none()
//         }

//         Message::TabNameSave(idx) => {
//             if let Some(tab_state) = app.tabs.get_mut(idx) {
//                 tab_state.is_editing_name = false;
//                 if tab_state.tab.name.trim().is_empty() {
//                     tab_state.tab.name = match &tab_state.content {
//                         WorkspaceContent::HttpRequest => "Untitled Request".to_string(),
//                         WorkspaceContent::CollectionRoot {
//                             collection_name, ..
//                         } => collection_name.clone(),
//                     };
//                 }
//             }
//             Task::none()
//         }

//         Message::EnvSelected(selected_name) => {
//             if let Some(name) = selected_name {
//                 app.active_env_index = app.environments.iter().position(|e| e.name == name);
//             } else {
//                 app.active_env_index = None;
//             }
//             Task::none()
//         }
//     }
// }

// fn view(app: &Rustrest) -> Element<Message> {
//     let env_options: Vec<String> = app.environments.iter().map(|e| e.name.clone()).collect();
//     let current_env_selection = app
//         .active_env_index
//         .and_then(|idx| app.environments.get(idx))
//         .map(|e| e.name.clone());

//     let env_selector = row![
//         pick_list(env_options, current_env_selection, |selected| {
//             Message::EnvSelected(Some(selected))
//         })
//         .placeholder("No Environment")
//         .width(Length::Fixed(180.0))
//     ]
//     .spacing(8)
//     .align_y(Alignment::Center);

//     let mut sidebar_contents = column![
//         button("Import Collection")
//             .on_press(Message::ImportCollectionPressed)
//             .padding(8)
//             .width(Length::Fill),
//         container(env_selector).padding(Padding {
//             top: 5.0,
//             right: 0.0,
//             bottom: 10.0,
//             left: 0.0,
//         }),
//     ]
//     .spacing(10);

//     if app.collections.is_empty() {
//         sidebar_contents = sidebar_contents.push(
//             text("No collections imported yet.")
//                 .size(11)
//                 .style(text::secondary),
//         );
//     } else {
//         for col in &app.collections {
//             let col_id = col.id;

//             let collection_header = button(
//                 text(format!("📁 {}", col.info.name))
//                     .font(Font {
//                         weight: iced::font::Weight::Bold,
//                         ..Font::DEFAULT
//                     })
//                     .size(15),
//             )
//             .on_press(Message::SidebarCollectionRootClicked(col_id))
//             .style(button::text)
//             .padding([4, 2]);

//             let mut col_tree = column![collection_header].spacing(4);

//             for item in &col.item {
//                 col_tree = render_sidebar_item(col_tree, item);
//             }
//             sidebar_contents = sidebar_contents.push(col_tree);
//         }
//     }

//     let sidebar = container(scrollable(sidebar_contents))
//         .width(Length::Fixed(260.0))
//         .height(Length::Fill)
//         .padding(10)
//         .style(container::bordered_box);

//     // main tabs workspace panel view
//     let mut tab_bar = row![].spacing(5).align_y(Alignment::Center);
//     for (idx, tab_state) in app.tabs.iter().enumerate() {
//         let is_active = idx == app.active_tab_index;
//         let tab = &tab_state.tab;

//         // build prefix badge depending on the workspace variant context
//         let prefix_badge = match &tab_state.content {
//             WorkspaceContent::HttpRequest => {
//                 let method_str = match &tab.method {
//                     HttpMethod::Custom(c) if c.trim().is_empty() => "CUSTOM".to_string(),
//                     HttpMethod::Custom(c) => c.to_uppercase(),
//                     other => format!("{}", other),
//                 };
//                 text(format!("[{}]", method_str)).size(11)
//             }
//             WorkspaceContent::CollectionRoot { .. } => {
//                 text("[COLL]").size(11).style(text::secondary)
//             }
//         };

//         let tab_content: Element<Message> = if tab_state.is_editing_name {
//             text_input("", &tab.name)
//                 .on_input(move |txt| Message::TabNameChanged(idx, txt))
//                 .on_submit(Message::TabNameSave(idx))
//                 .size(13)
//                 .width(Length::Fixed(100.0))
//                 .into()
//         } else {
//             button(text(&tab.name).size(13))
//                 .on_press(Message::TabNameDoubleClick(idx))
//                 .style(button::text)
//                 .padding(0)
//                 .into()
//         };

//         let mut tab_button = button(
//             row![
//                 prefix_badge,
//                 tab_content,
//                 button("×")
//                     .on_press(Message::CloseTabPressed(idx))
//                     .padding(2)
//                     .style(button::text)
//             ]
//             .spacing(6)
//             .align_y(Alignment::Center),
//         )
//         .padding(6);

//         if !is_active {
//             tab_button = tab_button
//                 .style(button::secondary)
//                 .on_press(Message::TabSelected(idx));
//         }
//         tab_bar = tab_bar.push(tab_button);
//     }

//     let add_tab_btn = button("+")
//         .on_press(Message::NewTabPressed)
//         .padding(6)
//         .style(button::success);

//     tab_bar = tab_bar.push(add_tab_btn);

//     // dynamic workbench rendering based on active tab state
//     let active_tab_state = &app.tabs[app.active_tab_index];
//     let tab_view: Element<Message> = match &active_tab_state.content {
//         WorkspaceContent::HttpRequest => active_tab_state
//             .tab
//             .view(Message::ActiveTabMessage, Message::SendPressed),
//         WorkspaceContent::CollectionRoot {
//             collection_id,
//             collection_name,
//         } => {
//             container(scrollable(
//                 column![
//                     text(collection_name).size(24).font(Font {
//                         weight: iced::font::Weight::Bold,
//                         ..Font::DEFAULT
//                     }),
//                     // text(format!("Collection ID: {}", collection_id))
//                     //     .size(12)
//                     //     .style(text::secondary),
//                     // section divider line
//                     container("")
//                         .height(Length::Fixed(1.0))
//                         .width(Length::Fill)
//                         .style(container::bordered_box),
//                     text("Collection Documentation").size(16).font(Font {
//                         weight: iced::font::Weight::Semibold,
//                         ..Font::DEFAULT
//                     }),
//                     text("Welcome to collection dashboard.").size(13),
//                 ]
//                 .spacing(15),
//             ))
//             .padding(20)
//             .width(Length::Fill)
//             .height(Length::Fill)
//             .into()
//         }
//     };

//     let workbench = column![tab_bar, tab_view].spacing(15);

//     row![sidebar, workbench]
//         .spacing(15)
//         .padding(15)
//         .width(Length::Fill)
//         .height(Length::Fill)
//         .into()
// }

// fn render_sidebar_item<'a>(
//     layout: iced::widget::Column<'a, Message>,
//     item: &'a CollectionItem,
// ) -> iced::widget::Column<'a, Message> {
//     match item {
//         CollectionItem::Folder {
//             name,
//             item: sub_items,
//         } => {
//             let mut folder_layout = column![text(format!("📁 {}", name)).size(15)]
//                 .spacing(3)
//                 .padding(Padding {
//                     top: 0.0,
//                     right: 0.0,
//                     bottom: 0.0,
//                     left: 10.0,
//                 });
//             for sub in sub_items {
//                 folder_layout = render_sidebar_item(folder_layout, sub);
//             }
//             layout.push(folder_layout)
//         }
//         CollectionItem::Request(req_node) => {
//             let req_clone = req_node.clone();
//             let label = format!("{} - {}", req_node.request.method, req_node.name);
//             layout.push(
//                 button(text(label).size(15))
//                     .on_press(Message::SidebarRequestClicked(req_clone))
//                     .style(button::text)
//                     .padding([2, 5]),
//             )
//         }
//     }
// }

// fn format_json_or_fallback(raw_body: &str) -> String {
//     if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
//         serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
//     } else {
//         format!("// Invalid JSON:\n{}", raw_body)
//     }
// }

// fn contains_request_node(items: &[CollectionItem], name: &str) -> bool {
//     for item in items {
//         match item {
//             CollectionItem::Request(node) => {
//                 if node.name == name {
//                     return true;
//                 }
//             }
//             CollectionItem::Folder {
//                 item: sub_items, ..
//             } => {
//                 if contains_request_node(sub_items, name) {
//                     return true;
//                 }
//             }
//         }
//     }
//     false
// }
