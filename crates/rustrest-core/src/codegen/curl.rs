//! cURL for POSIX shells (bash, zsh, Git Bash, WSL).

use super::{CodeGenerator, CodegenBody, CodegenRequest, FormPart, FormPartValue};
use super::{GeneratorOption, OptionValues};

pub struct Curl;

const MULTILINE: GeneratorOption = GeneratorOption {
    key: "multiline",
    label: "Multiline",
    description: "Put each option on its own line, joined with `\\`",
    default: true,
};

const LONG_FLAGS: GeneratorOption = GeneratorOption {
    key: "long_flags",
    label: "Long-form flags",
    description: "Use --header instead of -H, and so on",
    default: true,
};

const FOLLOW_REDIRECTS: GeneratorOption = GeneratorOption {
    key: "follow_redirects",
    label: "Follow redirects",
    description: "Add --location so cURL follows 3xx responses",
    default: true,
};

const TRIM_BODY: GeneratorOption = GeneratorOption {
    key: "trim_body",
    label: "Trim body",
    description: "Strip leading and trailing whitespace from a raw body",
    default: false,
};

const OPTIONS: &[GeneratorOption] = &[MULTILINE, LONG_FLAGS, FOLLOW_REDIRECTS, TRIM_BODY];

impl CodeGenerator for Curl {
    fn id(&self) -> &'static str {
        "curl"
    }

    fn label(&self) -> &'static str {
        "cURL"
    }

    fn syntax(&self) -> &'static str {
        "sh"
    }

    fn options(&self) -> &'static [GeneratorOption] {
        OPTIONS
    }

    fn generate(&self, request: &CodegenRequest, options: &OptionValues) -> String {
        let long = options.get(&LONG_FLAGS);
        let flag = |short: &'static str, long_form: &'static str| {
            if long { long_form } else { short }
        };

        // `curl [--location] [--request M] 'url'` stays on the first line,
        // every header/body option goes on its own.
        let mut head = vec!["curl".to_string()];
        if options.get(&FOLLOW_REDIRECTS) {
            head.push(flag("-L", "--location").to_string());
        }
        let has_body = request.body != CodegenBody::None;
        match request.method.as_str() {
            // `--request HEAD` makes cURL wait for a body that never comes
            "HEAD" => head.push(flag("-I", "--head").to_string()),
            // implied by cURL: GET without data, POST once there is data
            "GET" if !has_body => {}
            "POST" if has_body => {}
            method => head.push(format!(
                "{} {}",
                flag("-X", "--request"),
                shell_word(method)
            )),
        }
        head.push(quote(&request.url));

        let mut args = Vec::new();
        for (key, value) in &request.headers {
            // `Name:` would make cURL drop the header; `Name;` sends it empty
            let header = if value.is_empty() {
                format!("{key};")
            } else {
                format!("{key}: {value}")
            };
            args.push(format!("{} {}", flag("-H", "--header"), quote(&header)));
        }
        if !request.cookies.is_empty() {
            let cookies = request
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={}", v.trim()))
                .collect::<Vec<_>>()
                .join("; ");
            args.push(format!("{} {}", flag("-b", "--cookie"), quote(&cookies)));
        }

        match &request.body {
            CodegenBody::None => {}
            CodegenBody::Raw(body) => {
                let body = if options.get(&TRIM_BODY) {
                    body.trim()
                } else {
                    body.as_str()
                };
                // unlike --data, --data-raw never reads `@file`
                args.push(format!("--data-raw {}", quote(body)));
            }
            CodegenBody::UrlEncoded(pairs) => {
                for (key, value) in pairs {
                    // cURL encodes only the part after `=`, so the name goes in pre-encoded
                    let key: String =
                        url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
                    args.push(format!(
                        "--data-urlencode {}",
                        quote(&format!("{key}={value}"))
                    ));
                }
            }
            CodegenBody::FormData(parts) => {
                for part in parts {
                    args.push(format!(
                        "{} {}",
                        flag("-F", "--form"),
                        quote(&form_arg(part))
                    ));
                }
            }
            CodegenBody::Binary(path) => {
                args.push(format!("--data-binary {}", quote(&format!("@{path}"))));
            }
        }

        let separator = if options.get(&MULTILINE) {
            " \\\n  "
        } else {
            " "
        };
        std::iter::once(head.join(" "))
            .chain(args)
            .collect::<Vec<_>>()
            .join(separator)
    }
}

/// one `-F` value in cURL's own syntax: `name="text"` or `name=@"path"`,
/// plus `;type=...`. Quoting the content stops cURL reading a text value
/// that starts with `@`/`<` as a file, and keeps `;` inside it literal.
fn form_arg(part: &FormPart) -> String {
    let content = match &part.value {
        FormPartValue::Text(text) => format!("\"{}\"", escape_form(text)),
        FormPartValue::File(path) => format!("@\"{}\"", escape_form(path)),
    };
    match &part.content_type {
        Some(content_type) => format!("{}={content};type={content_type}", part.name),
        None => format!("{}={content}", part.name),
    }
}

fn escape_form(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// single-quotes `s` for a POSIX shell; a `'` inside becomes `'\''`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// leaves plain tokens bare, quoting anything a shell would interpret.
fn shell_word(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if plain { s.to_string() } else { quote(s) }
}
