use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSubTab {
    Params,
    Auth,
    Headers,
    Body,
    Cookies,
    Scripts,
}

impl RequestSubTab {
    pub const ALL: [Self; 6] = [
        Self::Params,
        Self::Auth,
        Self::Headers,
        Self::Body,
        Self::Cookies,
        Self::Scripts,
    ];

    pub fn name(&self) -> &str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Authorization",
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Cookies => "Cookies",
            Self::Scripts => "Scripts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTab {
    PreRequest,
    PostResponse,
}

impl ScriptTab {
    pub const ALL: [Self; 2] = [Self::PreRequest, Self::PostResponse];

    pub fn label(&self) -> &str {
        match self {
            Self::PreRequest => "Pre-request",
            Self::PostResponse => "Post-response",
        }
    }
}

pub use rustrest_core::{BodyType, FormDataRow, FormDataType, KeyValuePair};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawType {
    Text,
    JavaScript,
    Json,
    Html,
    Xml,
}

impl RawType {
    pub const ALL: [Self; 5] = [
        Self::Text,
        Self::JavaScript,
        Self::Json,
        Self::Html,
        Self::Xml,
    ];
}

impl std::fmt::Display for RawType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "Text"),
            Self::JavaScript => write!(f, "JavaScript"),
            Self::Json => write!(f, "JSON"),
            Self::Html => write!(f, "HTML"),
            Self::Xml => write!(f, "XML"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseView {
    Raw,
    Json,
}

impl ResponseView {
    pub const ALL: [ResponseView; 2] = [ResponseView::Raw, ResponseView::Json];

    pub fn label(&self) -> &str {
        match self {
            ResponseView::Raw => "Raw",
            ResponseView::Json => "JSON",
        }
    }
}

impl fmt::Display for ResponseView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseSubTab {
    Body,
    Cookies,
    Headers,
    TestResults,
}

impl ResponseSubTab {
    pub const ALL: [ResponseSubTab; 4] = [
        ResponseSubTab::Body,
        ResponseSubTab::Cookies,
        ResponseSubTab::Headers,
        ResponseSubTab::TestResults,
    ];

    pub fn label(&self) -> &str {
        match self {
            ResponseSubTab::Body => "Body",
            ResponseSubTab::Cookies => "Cookies",
            ResponseSubTab::Headers => "Headers",
            ResponseSubTab::TestResults => "Test Results",
        }
    }
}
