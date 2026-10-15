use serde::{Deserialize, Serialize};

/// A small, serializable widget tree a plugin returns to describe a sidebar
/// panel. panel UIs are declarative primitives the host renders with real widgets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiNode {
    Label(String),
    /// a dim, secondary line - hints, timestamps, role labels, or anything
    /// else that should read as lower-emphasis than a plain `Label`.
    Muted(String),
    /// markdown text, rendered by the host (headings, lists, code blocks, links, ...)
    Markdown(String),
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
    /// a compact menu button showing `label`.
    Dropdown {
        label: String,
        items: Vec<DropdownItem>,
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
    /// renders `child` with `style` applied.
    Styled {
        style: UiStyle,
        child: Box<UiNode>,
    },
}

/// one entry in a `UiNode::Dropdown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropdownItem {
    /// sent back as `UiEvent::Clicked(id)` when picked.
    pub id: String,
    pub label: String,
}

impl DropdownItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl std::fmt::Display for DropdownItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

impl UiNode {
    /// wraps this node in a `UiNode::Styled` - e.g.
    /// `UiNode::Button { .. }.styled(UiStyle::new().variant(ButtonVariant::Danger))`.
    pub fn styled(self, style: UiStyle) -> UiNode {
        UiNode::Styled {
            style,
            child: Box::new(self),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiStyle {
    /// text size in logical pixels (labels, button/checkbox labels, inputs,
    /// markdown body text).
    pub text_size: Option<f32>,
    /// text color, from the active theme's palette.
    pub tone: Option<TextTone>,
    /// bold text.
    pub bold: Option<bool>,
    pub width: Option<UiLength>,
    pub height: Option<UiLength>,
    /// `[vertical, horizontal]` padding in logical pixels.
    pub padding: Option<[f32; 2]>,
    /// gap between a `Row`/`Column`'s children (default 8).
    pub spacing: Option<f32>,
    /// button look - overrides the button's own `primary` flag.
    pub variant: Option<ButtonVariant>,
    /// renders buttons, inputs, and checkboxes non-interactive.
    pub disabled: Option<bool>,
    /// masks a `TextInput`'s value, for passwords/API keys.
    pub secure: Option<bool>,
}

impl UiStyle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = Some(size);
        self
    }
    pub fn tone(mut self, tone: TextTone) -> Self {
        self.tone = Some(tone);
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = Some(true);
        self
    }
    pub fn width(mut self, width: UiLength) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: UiLength) -> Self {
        self.height = Some(height);
        self
    }
    pub fn padding(mut self, vertical: f32, horizontal: f32) -> Self {
        self.padding = Some([vertical, horizontal]);
        self
    }
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }
    pub fn secure(mut self) -> Self {
        self.secure = Some(true);
        self
    }

    /// `other`'s set fields on top of `self`'s.
    pub fn overlay(self, other: UiStyle) -> Self {
        Self {
            text_size: other.text_size.or(self.text_size),
            tone: other.tone.or(self.tone),
            bold: other.bold.or(self.bold),
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            padding: other.padding.or(self.padding),
            spacing: other.spacing.or(self.spacing),
            variant: other.variant.or(self.variant),
            disabled: other.disabled.or(self.disabled),
            secure: other.secure.or(self.secure),
        }
    }

    /// just the fields that cascade from a `Row`/`Column` into its children.
    pub fn inherited(self) -> Self {
        Self {
            text_size: self.text_size,
            tone: self.tone,
            bold: self.bold,
            disabled: self.disabled,
            ..Self::default()
        }
    }
}

/// a theme-palette text color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextTone {
    Default,
    Muted,
    Primary,
    Success,
    Warning,
    Danger,
}

/// a button's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Success,
    Warning,
    Danger,
    /// borderless, background only on hover - for toolbar/icon buttons.
    Text,
}

/// a widget dimension.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiLength {
    /// take all available space.
    Fill,
    /// take only as much space as the content needs.
    Shrink,
    /// exactly this many logical pixels.
    Fixed(f32),
}

/// A user interaction with a previously rendered `UiNode` tree, identified by
/// the `id` of the widget the user interacted with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
