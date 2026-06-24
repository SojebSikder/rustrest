#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSubTab {
    Params,
    Auth,
    Headers,
    Body,
}

impl RequestSubTab {
    pub const ALL: [Self; 4] = [Self::Params, Self::Auth, Self::Headers, Self::Body];

    pub fn name(&self) -> &str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Authorization",
            Self::Headers => "Headers",
            Self::Body => "Body",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    None,
    FormData,
    XWwwFormUrlencoded,
    Raw,
    Binary,
}

impl BodyType {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::FormData,
        Self::XWwwFormUrlencoded,
        Self::Raw,
        Self::Binary,
    ];

    pub fn label(&self) -> &str {
        match self {
            Self::None => "none",
            Self::FormData => "form-data",
            Self::XWwwFormUrlencoded => "x-www-form-urlencoded",
            Self::Raw => "raw",
            Self::Binary => "binary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValuePair {
    pub is_active: bool,
    pub key: String,
    pub value: String,
}

impl KeyValuePair {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            is_active: true,
            key: String::from(key),
            value: String::from(value),
        }
    }
}

// body type

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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "Text"),
            Self::JavaScript => write!(f, "JavaScript"),
            Self::Json => write!(f, "JSON"),
            Self::Html => write!(f, "HTML"),
            Self::Xml => write!(f, "XML"),
        }
    }
}

// response view type
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
