use crate::message::Message;
use iced::keyboard::{Key, Modifiers};
use iced::{Event, Subscription, event};

/// declarative binding of a key + modifier combo to a `Message`. add an entry
/// to `bindings()` to wire up a new shortcut
struct Shortcut {
    key: &'static str,
    command: bool,
    shift: bool,
    alt: bool,
    message: Message,
}

fn bindings() -> Vec<Shortcut> {
    vec![
        Shortcut {
            key: "s",
            command: true,
            shift: false,
            alt: false,
            message: Message::SaveActiveRequestShortcut,
        },
        Shortcut {
            key: "w",
            command: true,
            shift: false,
            alt: false,
            message: Message::CloseActiveTabShortcut,
        },
    ]
}

pub fn subscription() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Character(ref c),
            modifiers,
            ..
        }) => bindings()
            .into_iter()
            .find(|s| s.key == c.as_str() && matches(modifiers, s))
            .map(|s| s.message),
        _ => None,
    })
}

fn matches(modifiers: Modifiers, shortcut: &Shortcut) -> bool {
    modifiers.command() == shortcut.command
        && modifiers.shift() == shortcut.shift
        && modifiers.alt() == shortcut.alt
}
