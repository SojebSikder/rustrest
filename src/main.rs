#![windows_subsystem = "windows"]

mod http_client;
mod tab;

use http_client::{HttpResponse, send_request};
use tab::{Tab, TabMessage};

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Application, Command, Element, Length, Settings, Size, Theme};

const APP_NAME: &str = "Rustrest";

pub fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(1100.0, 800.0);
    Rustrest::run(settings)
}

struct Rustrest {
    tabs: Vec<Tab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

#[derive(Debug, Clone)]
enum Message {
    TabSelected(usize),
    NewTabPressed,
    CloseTabPressed(usize),
    ActiveTabMessage(TabMessage),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>),
}

impl Application for Rustrest {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                tabs: vec![Tab::new(1)],
                active_tab_index: 0,
                next_tab_id: 2,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        format!("{} - API Testing Platform", APP_NAME)
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TabSelected(index) => {
                if index < self.tabs.len() {
                    self.active_tab_index = index;
                }
                Command::none()
            }
            Message::NewTabPressed => {
                self.tabs.push(Tab::new(self.next_tab_id));
                self.active_tab_index = self.tabs.len() - 1;
                self.next_tab_id += 1;
                Command::none()
            }
            Message::CloseTabPressed(index) => {
                if self.tabs.len() > 1 {
                    self.tabs.remove(index);
                    if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.tabs.len() - 1;
                    }
                }
                Command::none()
            }
            Message::ActiveTabMessage(tab_msg) => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.update(tab_msg);
                }
                Command::none()
            }
            Message::SendPressed => {
                let tab_idx = self.active_tab_index;
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    if tab.is_loading || tab.url.is_empty() {
                        return Command::none();
                    }
                    tab.is_loading = true;
                    tab.response = None;

                    return Command::perform(
                        send_request(
                            tab.url.clone(),
                            tab.method,
                            tab.request_body.clone(),
                            tab.request_headers.clone(),
                        ),
                        move |res| Message::ResponseReceived(tab_idx, res),
                    );
                }
                Command::none()
            }
            Message::ResponseReceived(tab_idx, res) => {
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    tab.is_loading = false;
                    tab.response = Some(res);
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        // Main Tab Selection Bar Layout
        let mut tab_bar = row![].spacing(5).align_items(Alignment::Center);
        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_active = idx == self.active_tab_index;
            let mut tab_button = button(
                row![
                    text(&tab.name).size(13),
                    button("×")
                        .on_press(Message::CloseTabPressed(idx))
                        .padding(2)
                        .style(iced::theme::Button::Text)
                ]
                .spacing(8)
                .align_items(Alignment::Center),
            )
            .padding(8);

            if !is_active {
                tab_button = tab_button
                    .style(iced::theme::Button::Secondary)
                    .on_press(Message::TabSelected(idx));
            }
            tab_bar = tab_bar.push(tab_button);
        }

        let add_tab_btn = button("+")
            .on_press(Message::NewTabPressed)
            .padding(8)
            .style(iced::theme::Button::Positive);
        tab_bar = tab_bar.push(add_tab_btn);

        // Render current active layout workspace
        let current_tab = &self.tabs[self.active_tab_index];
        let tab_view = current_tab.view(Message::ActiveTabMessage, Message::SendPressed);

        container(column![tab_bar, tab_view].spacing(18))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(25)
            .into()
    }
}
