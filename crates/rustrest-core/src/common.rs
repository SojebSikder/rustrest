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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDataType {
    Text,
    File,
}

impl FormDataType {
    pub const ALL: [Self; 2] = [Self::Text, Self::File];
}

impl std::fmt::Display for FormDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "Text"),
            Self::File => write!(f, "File"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormDataRow {
    pub is_active: bool,
    pub key: String,
    pub value: String,
    pub field_type: FormDataType,
}

impl FormDataRow {
    pub fn new(key: &str, value: &str, field_type: FormDataType) -> Self {
        Self {
            is_active: true,
            key: String::from(key),
            value: String::from(value),
            field_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
