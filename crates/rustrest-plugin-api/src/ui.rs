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
    },
    Checkbox {
        id: String,
        label: String,
        checked: bool,
    },
    List(Vec<String>),
    Row(Vec<UiNode>),
    Column(Vec<UiNode>),
    /// flexible blank space that expands to fill whatever room is left in
    /// its parent `Row`/`Column` - e.g. pushing a header's icon button to
    /// the opposite end from a label it's sharing a row with.
    Spacer,
}

/// A user interaction with a previously rendered `UiNode` tree, identified by
/// the `id` of the widget the user interacted with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
