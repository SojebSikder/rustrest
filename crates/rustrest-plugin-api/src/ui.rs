use serde::{Deserialize, Serialize};

/// A small, serializable widget tree a plugin returns to describe a sidebar
/// panel. panel UIs are declarative primitives the host renders with real widgets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiNode {
    Label(String),
    /// a dim, secondary line - hints, timestamps, role labels, or anything
    /// else that should read as lower-emphasis than a plain `Label`.
    Muted(String),
    Button {
        id: String,
        label: String,
        /// true renders this as the emphasized/primary action on screen
        /// (e.g. a form's one "Send" or "Save" button); false (the common
        /// case) renders it as a plain secondary button.
        #[serde(default)]
        primary: bool,
    },
    TextInput {
        id: String,
        value: String,
        placeholder: String,
        /// if set, pressing Enter in this field fires `UiEvent::Clicked`
        /// with this id, as if that button had been pressed - e.g. wiring
        /// a chat input to its Send button's id so Enter submits without
        /// reaching for the mouse.
        #[serde(default)]
        on_submit: Option<String>,
    },
    Checkbox {
        id: String,
        label: String,
        checked: bool,
    },
    List(Vec<String>),
    Row(Vec<UiNode>),
    Column(Vec<UiNode>),
    /// Fills horizontal space (Length::Fill width, 0 height)
    HorizontalSpacer,
    /// Fills vertical space (0 width, Length::Fill height)
    VerticalSpacer,
    FixedSpace {
        width: f32,
        height: f32,
    }, // Fixed dimensions
    /// marks this subtree as an independent scroll region. The host only
    /// auto-scrolls a plugin's whole panel when it contains no explicit
    /// `Scrollable` anywhere; once a plugin uses one, it's opting into
    /// controlling scrolling itself - typically to keep a header and/or
    /// footer (e.g. a settings button, a send box) pinned in place while
    /// just the middle section scrolls.
    Scrollable(Box<UiNode>),
    /// like `Scrollable`, but the host snaps it to the bottom every time
    /// this panel is re-rendered - for a chat-style feed where new content
    /// (a sent message, a streamed reply) should always be immediately
    /// visible without the user having to scroll down themselves.
    AutoScroll(Box<UiNode>),
}

/// A user interaction with a previously rendered `UiNode` tree, identified by
/// the `id` of the widget the user interacted with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
