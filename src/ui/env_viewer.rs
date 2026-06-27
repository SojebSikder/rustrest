use crate::env::Environment;
use iced::widget::{column, pick_list, row, text};
use iced::{Alignment, Element, Length};

pub fn render_environment_bar<'a, Message>(
    environments: &'a [Environment],
    active_env_index: Option<usize>,
    on_select: impl Fn(Option<usize>) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    // generate label selectors list for pick_list option display
    let env_names: Vec<String> = environments.iter().map(|e| e.name.clone()).collect();

    row![
        text("Environment:").size(14),
        pick_list(
            env_names,
            active_env_index.map(|idx| environments[idx].name.clone()),
            move |selected_name| {
                let position = environments.iter().position(|e| e.name == selected_name);
                on_select(position)
            }
        )
        .placeholder("No Environment")
        .width(Length::Fixed(200.0))
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
