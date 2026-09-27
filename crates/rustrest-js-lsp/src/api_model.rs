//! Static type graph of the script API registered by
//! `rustrest_core::script_engine::ScriptRunner`, plus the common JS globals.

use crate::document::DocKind;

/// Which script kinds expose a member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Avail {
    Both,
    PreRequest,
    PostResponse,
}

/// A function parameter; optional params carry a trailing `?` in `name`.
#[derive(Debug)]
pub struct Param {
    pub name: &'static str,
    pub ty: &'static str,
}

/// A property or method of a [`TypeDef`].
#[derive(Debug)]
pub struct Member {
    pub name: &'static str,
    /// `Some` for callable members.
    pub params: Option<&'static [Param]>,
    /// property type, or return type for methods; resolvable via [`type_def`]
    /// when it names a type in the graph
    pub ty: &'static str,
    pub doc: &'static str,
    pub avail: Avail,
}

/// A named object type with a fixed member list.
#[derive(Debug)]
pub struct TypeDef {
    pub name: &'static str,
    /// closed types warn on unknown static member access
    pub closed: bool,
    pub members: &'static [Member],
}

macro_rules! params {
    ($($name:literal : $ty:literal),* $(,)?) => {
        &[$(Param { name: $name, ty: $ty }),*]
    };
}

const fn prop(name: &'static str, ty: &'static str, doc: &'static str) -> Member {
    Member {
        name,
        params: None,
        ty,
        doc,
        avail: Avail::Both,
    }
}

const fn method(
    name: &'static str,
    params: &'static [Param],
    ty: &'static str,
    doc: &'static str,
) -> Member {
    Member {
        name,
        params: Some(params),
        ty,
        doc,
        avail: Avail::Both,
    }
}

impl Member {
    const fn pre(mut self) -> Self {
        self.avail = Avail::PreRequest;
        self
    }

    const fn post(mut self) -> Self {
        self.avail = Avail::PostResponse;
        self
    }

    /// Whether the member exists in scripts of `kind` (`Unknown` sees everything).
    pub fn available_in(&self, kind: DocKind) -> bool {
        match (self.avail, kind) {
            (Avail::Both, _) | (_, DocKind::Unknown) => true,
            (Avail::PreRequest, k) => k == DocKind::PreRequest,
            (Avail::PostResponse, k) => k == DocKind::PostResponse,
        }
    }

    pub fn is_function(&self) -> bool {
        self.params.is_some()
    }

    /// `(a: T, b?: U)` for methods, empty for properties.
    pub fn params_sig(&self) -> String {
        match self.params {
            Some(params) => {
                let list: Vec<String> = params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.ty))
                    .collect();
                format!("({})", list.join(", "))
            }
            None => String::new(),
        }
    }

    /// Short completion detail: `(key: string) => void` or the property type.
    pub fn detail(&self) -> String {
        if self.is_function() {
            format!("{} => {}", self.params_sig(), self.ty)
        } else {
            self.ty.to_string()
        }
    }

    /// Full signature, e.g. `pm.environment.set(key: string, value: string): void`.
    pub fn signature(&self, qualifier: &str) -> String {
        let name = if qualifier.is_empty() {
            self.name.to_string()
        } else {
            format!("{qualifier}.{}", self.name)
        };
        format!("{name}{}: {}", self.params_sig(), self.ty)
    }

    /// Description plus an availability note for kind-restricted members.
    pub fn full_doc(&self) -> String {
        match self.avail {
            Avail::Both => self.doc.to_string(),
            Avail::PreRequest => format!("{} (pre-request scripts only)", self.doc),
            Avail::PostResponse => format!("{} (post-response scripts only)", self.doc),
        }
    }
}

impl TypeDef {
    pub fn member(&self, name: &str) -> Option<&'static Member> {
        self.members.iter().find(|m| m.name == name)
    }
}

/// Looks up a type of the graph by name.
pub fn type_def(name: &str) -> Option<&'static TypeDef> {
    ALL_TYPES.iter().copied().find(|t| t.name == name)
}

static ALL_TYPES: &[&TypeDef] = &[
    &GLOBAL,
    &PM,
    &CONSOLE,
    &VARIABLE_SCOPE,
    &RESPONSE,
    &RESPONSE_HEADERS,
    &RESPONSE_ASSERTION,
    &ASSERTION,
    &JSON,
    &MATH,
    &OBJECT_CTOR,
    &ARRAY_CTOR,
    &DATE_CTOR,
    &NUMBER_CTOR,
];

/// Top-level scope: `pm`, `console` and the common JS globals.
pub static GLOBAL: TypeDef = TypeDef {
    name: "globalThis",
    closed: false,
    members: &[
        prop(
            "pm",
            "Pm",
            "Rustrest scripting API: variables, request/response helpers and tests.",
        ),
        prop(
            "console",
            "Console",
            "Console output, shown in the script log.",
        ),
        prop("JSON", "JSON", "JSON parsing and serialization."),
        prop("Math", "Math", "Mathematical constants and functions."),
        prop("Date", "DateConstructor", "Date and time values."),
        prop("Object", "ObjectConstructor", "Object helpers."),
        prop("Array", "ArrayConstructor", "Array helpers."),
        prop(
            "String",
            "StringConstructor",
            "String conversion and helpers.",
        ),
        prop(
            "Number",
            "NumberConstructor",
            "Number conversion and helpers.",
        ),
        prop("Boolean", "BooleanConstructor", "Boolean conversion."),
        prop("Promise", "PromiseConstructor", "Asynchronous values."),
        method(
            "parseInt",
            params!["string": "string", "radix?": "number"],
            "number",
            "Parses a string into an integer.",
        ),
        method(
            "parseFloat",
            params!["string": "string"],
            "number",
            "Parses a string into a floating-point number.",
        ),
        method(
            "isNaN",
            params!["value": "any"],
            "boolean",
            "Returns true if the value is NaN after number conversion.",
        ),
        method(
            "isFinite",
            params!["value": "any"],
            "boolean",
            "Returns true if the value is a finite number after number conversion.",
        ),
        method(
            "encodeURIComponent",
            params!["uriComponent": "string"],
            "string",
            "Percent-encodes a URI component.",
        ),
        method(
            "decodeURIComponent",
            params!["encodedURIComponent": "string"],
            "string",
            "Decodes a percent-encoded URI component.",
        ),
        method(
            "encodeURI",
            params!["uri": "string"],
            "string",
            "Percent-encodes a full URI.",
        ),
        method(
            "decodeURI",
            params!["encodedURI": "string"],
            "string",
            "Decodes a percent-encoded full URI.",
        ),
        prop("undefined", "undefined", "The undefined value."),
        prop("NaN", "number", "Not-a-Number."),
        prop("Infinity", "number", "Positive infinity."),
    ],
};

/// The `pm` object; members differ between pre-request and post-response.
pub static PM: TypeDef = TypeDef {
    name: "Pm",
    closed: true,
    members: &[
        method(
            "getVariable",
            params!["key": "string"],
            "string | undefined",
            "Returns the value of variable `key` (\"\" in pre-request, undefined in post-response when unset).",
        ),
        method(
            "setVariable",
            params!["key": "string", "value": "string"],
            "void",
            "Sets variable `key` to `value` (stored as a string).",
        ),
        method(
            "setHeader",
            params!["key": "string", "value": "string"],
            "void",
            "Sets a header on the outgoing request.",
        )
        .pre(),
        method(
            "getResponseBody",
            params![],
            "string",
            "Returns the raw response body.",
        )
        .post(),
        method(
            "getStatus",
            params![],
            "number",
            "Returns the response status code.",
        )
        .post(),
        prop(
            "environment",
            "VariableScope",
            "Environment variables (same store as pm.variables).",
        ),
        prop(
            "variables",
            "VariableScope",
            "Request variables (same store as pm.environment).",
        ),
        prop(
            "globals",
            "VariableScope",
            "Global variables, shared across requests.",
        ),
        prop(
            "response",
            "Response",
            "The response of the executed request.",
        )
        .post(),
        method(
            "sendRequest",
            params![
                "urlOrOptions": "string | { url: string, method?: string, header?: { key: string, value: string }[], headers?: object, body?: string | { raw: string } }",
                "callback?": "(err: string | null, res: Response | null) => void",
            ],
            "void",
            "Sends an HTTP request synchronously, then calls `callback(err, res)`; `res` has the same shape as pm.response.",
        ),
        method(
            "test",
            params!["name": "string", "fn": "() => void"],
            "void",
            "Runs `fn` as a named test; it fails if `fn` throws (e.g. a failed pm.expect).",
        )
        .post(),
        method(
            "expect",
            params!["value": "any"],
            "Assertion",
            "Starts a chai-style assertion on `value`.",
        )
        .post(),
    ],
};

pub static CONSOLE: TypeDef = TypeDef {
    name: "Console",
    closed: true,
    members: &[
        method(
            "log",
            params!["...args": "any[]"],
            "void",
            "Logs the arguments (objects as JSON) to the script log.",
        ),
        method(
            "info",
            params!["...args": "any[]"],
            "void",
            "Logs the arguments at info level.",
        ),
        method(
            "warn",
            params!["...args": "any[]"],
            "void",
            "Logs the arguments at warn level.",
        ),
        method(
            "error",
            params!["...args": "any[]"],
            "void",
            "Logs the arguments at error level.",
        ),
    ],
};

/// `pm.environment` / `pm.variables` / `pm.globals`.
pub static VARIABLE_SCOPE: TypeDef = TypeDef {
    name: "VariableScope",
    closed: true,
    members: &[
        method(
            "get",
            params!["key": "string"],
            "string | undefined",
            "Returns the value of `key`, or undefined when unset.",
        ),
        method(
            "set",
            params!["key": "string", "value": "string"],
            "void",
            "Sets `key` to `value` (stored as a string).",
        ),
        method(
            "has",
            params!["key": "string"],
            "boolean",
            "Returns true if `key` is set.",
        ),
        method("unset", params!["key": "string"], "void", "Removes `key`."),
        method("clear", params![], "void", "Removes all variables."),
    ],
};

/// `pm.response`, and the `res` passed to a `pm.sendRequest` callback.
pub static RESPONSE: TypeDef = TypeDef {
    name: "Response",
    closed: true,
    members: &[
        prop("code", "number", "HTTP status code, e.g. 200."),
        prop("status", "string", "HTTP status text, e.g. \"OK\"."),
        prop("body", "string", "Raw response body."),
        prop(
            "headers",
            "ResponseHeaders",
            "Response headers keyed by name, plus a case-insensitive `get(name)`.",
        ),
        method(
            "json",
            params![],
            "any",
            "Parses the body as JSON (throws if it is not valid JSON).",
        ),
        method("text", params![], "string", "Returns the body as text."),
        prop(
            "to",
            "ResponseAssertion",
            "Response assertions: `to.have.status(200)`, `to.have.header(name)`, `to.be.ok`.",
        ),
    ],
};

/// `pm.response.headers`; open because header names are also properties.
pub static RESPONSE_HEADERS: TypeDef = TypeDef {
    name: "ResponseHeaders",
    closed: false,
    members: &[method(
        "get",
        params!["name": "string"],
        "string | undefined",
        "Returns the header value (case-insensitive name), or undefined.",
    )],
};

const RESPONSE_CHAIN_DOC: &str = "Chain word; returns the same response assertion.";

/// `pm.response.to.*`.
pub static RESPONSE_ASSERTION: TypeDef = TypeDef {
    name: "ResponseAssertion",
    closed: true,
    members: &[
        method(
            "status",
            params!["expected": "number | string"],
            "void",
            "Asserts the status code (number) or status text (string).",
        ),
        method(
            "header",
            params!["name": "string", "value?": "string"],
            "void",
            "Asserts a header is present (case-insensitive), optionally with `value`.",
        ),
        method(
            "body",
            params!["substring?": "string"],
            "void",
            "Asserts the body contains `substring`, or is non-empty when omitted.",
        ),
        prop("ok", "void", "Asserts a 2xx status."),
        prop(
            "not",
            "ResponseAssertion",
            "Negates the following assertion.",
        ),
        prop("to", "ResponseAssertion", RESPONSE_CHAIN_DOC),
        prop("have", "ResponseAssertion", RESPONSE_CHAIN_DOC),
        prop("be", "ResponseAssertion", RESPONSE_CHAIN_DOC),
        prop("and", "ResponseAssertion", RESPONSE_CHAIN_DOC),
        prop("with", "ResponseAssertion", RESPONSE_CHAIN_DOC),
    ],
};

const CHAIN_DOC: &str = "Chain word; returns the same assertion.";

/// The chai-style object returned by `pm.expect(value)`.
pub static ASSERTION: TypeDef = TypeDef {
    name: "Assertion",
    closed: true,
    members: &[
        method(
            "equal",
            params!["expected": "any"],
            "void",
            "Asserts the value equals `expected` (numbers by value, others by JSON).",
        ),
        method(
            "eql",
            params!["expected": "any"],
            "void",
            "Asserts the value deeply equals `expected`.",
        ),
        method(
            "a",
            params!["type": "string"],
            "void",
            "Asserts the value's type (\"string\", \"number\", \"object\", \"array\", \"null\", ...).",
        ),
        method(
            "an",
            params!["type": "string"],
            "void",
            "Asserts the value's type (\"string\", \"number\", \"object\", \"array\", \"null\", ...).",
        ),
        method(
            "include",
            params!["needle": "any"],
            "void",
            "Asserts a string contains `needle`, or an array contains an equal element.",
        ),
        method(
            "contain",
            params!["needle": "any"],
            "void",
            "Alias of `include`.",
        ),
        method(
            "property",
            params!["name": "string"],
            "void",
            "Asserts the object has property `name`.",
        ),
        method(
            "lengthOf",
            params!["length": "number"],
            "void",
            "Asserts the string/array length.",
        ),
        method(
            "above",
            params!["n": "number"],
            "void",
            "Asserts the value is greater than `n`.",
        ),
        method(
            "below",
            params!["n": "number"],
            "void",
            "Asserts the value is less than `n`.",
        ),
        method(
            "least",
            params!["n": "number"],
            "void",
            "Asserts the value is greater than or equal to `n`.",
        ),
        method(
            "most",
            params!["n": "number"],
            "void",
            "Asserts the value is less than or equal to `n`.",
        ),
        prop("true", "void", "Asserts the value is `true`."),
        prop("false", "void", "Asserts the value is `false`."),
        prop("null", "void", "Asserts the value is `null`."),
        prop("undefined", "void", "Asserts the value is `undefined`."),
        prop(
            "exist",
            "void",
            "Asserts the value is neither null nor undefined.",
        ),
        prop("ok", "void", "Asserts the value is truthy."),
        prop("empty", "void", "Asserts the string/array is empty."),
        prop("not", "Assertion", "Negates the following assertion."),
        prop("to", "Assertion", CHAIN_DOC),
        prop("be", "Assertion", CHAIN_DOC),
        prop("been", "Assertion", CHAIN_DOC),
        prop("is", "Assertion", CHAIN_DOC),
        prop("that", "Assertion", CHAIN_DOC),
        prop("which", "Assertion", CHAIN_DOC),
        prop("and", "Assertion", CHAIN_DOC),
        prop("has", "Assertion", CHAIN_DOC),
        prop("have", "Assertion", CHAIN_DOC),
        prop("with", "Assertion", CHAIN_DOC),
        prop("at", "Assertion", CHAIN_DOC),
        prop("of", "Assertion", CHAIN_DOC),
        prop("same", "Assertion", CHAIN_DOC),
        prop("but", "Assertion", CHAIN_DOC),
        prop("does", "Assertion", CHAIN_DOC),
    ],
};

pub static JSON: TypeDef = TypeDef {
    name: "JSON",
    closed: false,
    members: &[
        method(
            "parse",
            params!["text": "string"],
            "any",
            "Parses a JSON string.",
        ),
        method(
            "stringify",
            params!["value": "any", "replacer?": "any", "space?": "string | number"],
            "string",
            "Serializes a value to a JSON string.",
        ),
    ],
};

pub static MATH: TypeDef = TypeDef {
    name: "Math",
    closed: false,
    members: &[
        prop(
            "PI",
            "number",
            "Ratio of a circle's circumference to its diameter.",
        ),
        prop("E", "number", "Euler's number."),
        method("abs", params!["x": "number"], "number", "Absolute value."),
        method("ceil", params!["x": "number"], "number", "Rounds up."),
        method("floor", params!["x": "number"], "number", "Rounds down."),
        method(
            "round",
            params!["x": "number"],
            "number",
            "Rounds to the nearest integer.",
        ),
        method(
            "trunc",
            params!["x": "number"],
            "number",
            "Drops the fractional part.",
        ),
        method(
            "sign",
            params!["x": "number"],
            "number",
            "Sign of x (-1, 0 or 1).",
        ),
        method(
            "max",
            params!["...values": "number[]"],
            "number",
            "Largest of the values.",
        ),
        method(
            "min",
            params!["...values": "number[]"],
            "number",
            "Smallest of the values.",
        ),
        method(
            "pow",
            params!["x": "number", "y": "number"],
            "number",
            "x to the power of y.",
        ),
        method("sqrt", params!["x": "number"], "number", "Square root."),
        method(
            "random",
            params![],
            "number",
            "Pseudo-random number in [0, 1).",
        ),
    ],
};

pub static OBJECT_CTOR: TypeDef = TypeDef {
    name: "ObjectConstructor",
    closed: false,
    members: &[
        method(
            "keys",
            params!["o": "object"],
            "string[]",
            "Own enumerable property names.",
        ),
        method(
            "values",
            params!["o": "object"],
            "any[]",
            "Own enumerable property values.",
        ),
        method(
            "entries",
            params!["o": "object"],
            "[string, any][]",
            "Own enumerable [key, value] pairs.",
        ),
        method(
            "assign",
            params!["target": "object", "...sources": "object[]"],
            "object",
            "Copies properties from `sources` into `target`.",
        ),
    ],
};

pub static ARRAY_CTOR: TypeDef = TypeDef {
    name: "ArrayConstructor",
    closed: false,
    members: &[method(
        "isArray",
        params!["value": "any"],
        "boolean",
        "Returns true if the value is an array.",
    )],
};

pub static DATE_CTOR: TypeDef = TypeDef {
    name: "DateConstructor",
    closed: false,
    members: &[method(
        "now",
        params![],
        "number",
        "Milliseconds since the Unix epoch.",
    )],
};

pub static NUMBER_CTOR: TypeDef = TypeDef {
    name: "NumberConstructor",
    closed: false,
    members: &[
        method(
            "isInteger",
            params!["value": "any"],
            "boolean",
            "Returns true if the value is an integer number.",
        ),
        method(
            "parseFloat",
            params!["string": "string"],
            "number",
            "Parses a string into a floating-point number.",
        ),
    ],
};

/// JS keywords offered at top level.
pub const KEYWORDS: &[&str] = &[
    "const",
    "let",
    "var",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "break",
    "continue",
    "switch",
    "case",
    "default",
    "try",
    "catch",
    "finally",
    "throw",
    "new",
    "delete",
    "typeof",
    "instanceof",
    "in",
    "of",
    "class",
    "this",
    "true",
    "false",
    "null",
    "async",
    "await",
];
