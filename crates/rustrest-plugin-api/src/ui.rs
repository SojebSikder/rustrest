use serde::{Deserialize, Serialize};

/// A small, serializable widget tree a plugin returns to describe a sidebar
/// panel. panel UIs are declarative primitives the host renders with real widgets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiNode {
    Label(String),
    Button {
        id: String,
        label: String,
    },
    TextInput {
        id: String,
        value: String,
        placeholder: String,
    },
    Checkbox {
        id: String,
        label: String,
        checked: bool,
    },
    List(Vec<String>),
    Row(Vec<UiNode>),
    Column(Vec<UiNode>),
}

/// A user interaction with a previously rendered `UiNode` tree, identified by
/// the `id` of the widget the user interacted with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
