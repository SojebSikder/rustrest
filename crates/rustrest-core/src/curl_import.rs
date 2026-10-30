//! Turns a pasted cURL command into a request

use crate::auth::{AuthType, RequestAuth};
use crate::collection::model::{
    PostmanBody, PostmanBodyFile, PostmanBodyRow, PostmanFileSrc, PostmanHeader,
    PostmanRequestDetails, PostmanUrl,
};

/// one `-d`-style argument, kept apart until we know how the body is sent.
enum DataArg {
    /// `-d`, `--data-raw`, `--data-binary`, `--json`: sent as written.
    Raw(String),
    /// `--data-urlencode`: cURL encodes the part after `=`.
    UrlEncode(String),
    /// `-d @path` / `--data-binary @path`.
    File(String),
}

/// options that take a value we don't use, so it isn't mistaken for the URL.
const SKIPPED_WITH_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-x",
    "--proxy",
    "-U",
    "--proxy-user",
    "--retry",
    "--retry-delay",
    "--retry-max-time",
    "-w",
    "--write-out",
    "--cacert",
    "--capath",
    "-E",
    "--cert",
    "--cert-type",
    "--key",
    "--key-type",
    "--resolve",
    "--connect-to",
    "--limit-rate",
    "-r",
    "--range",
    "-c",
    "--cookie-jar",
    "-D",
    "--dump-header",
    "--interface",
    "--max-redirs",
    "-y",
    "--speed-time",
    "-Y",
    "--speed-limit",
    "--trace",
    "--trace-ascii",
    "--config",
    "-K",
    "--aws-sigv4",
    "--unix-socket",
    "--expect100-timeout",
    "--keepalive-time",
    "--happy-eyeballs-timeout-ms",
];

/// short options that take a value, so `-XPOST` and `-X POST` both work.
const SHORT_WITH_VALUE: &str = "XHdFAebuTomxUwEcDrKyY";

pub fn parse(command: &str) -> Result<PostmanRequestDetails, String> {
    let tokens = tokenize(command)?;
    let mut tokens = tokens.into_iter().peekable();
    match tokens.next() {
        Some(first) if is_curl(&first) => {}
        _ => return Err("Expected a command starting with `curl`".to_string()),
    }

    let mut url: Option<String> = None;
    let mut method: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut data: Vec<DataArg> = Vec::new();
    let mut form: Vec<PostmanBodyRow> = Vec::new();
    let mut upload_file: Option<String> = None;
    let mut user: Option<String> = None;
    let mut bearer: Option<String> = None;
    let mut cookies: Vec<String> = Vec::new();
    let mut head = false;
    let mut get = false;
    let mut json = false;

    while let Some(token) = tokens.next() {
        // split `-XPOST` / `-sSL` into individual options
        let options: Vec<(String, Option<String>)> =
            if token.starts_with("--") || token.len() < 2 || !token.starts_with('-') {
                vec![(token, None)]
            } else {
                let mut out = Vec::new();
                let chars: Vec<char> = token[1..].chars().collect();
                for (i, c) in chars.iter().enumerate() {
                    if SHORT_WITH_VALUE.contains(*c) {
                        let rest: String = chars[i + 1..].iter().collect();
                        out.push((format!("-{c}"), (!rest.is_empty()).then_some(rest)));
                        break;
                    }
                    out.push((format!("-{c}"), None));
                }
                out
            };

        for (option, attached) in options {
            let mut value = || -> Result<String, String> {
                match attached.clone() {
                    Some(v) => Ok(v),
                    None => tokens
                        .next()
                        .ok_or_else(|| format!("Missing value for `{option}`")),
                }
            };

            match option.as_str() {
                "-X" | "--request" => method = Some(value()?.to_uppercase()),
                "-H" | "--header" => {
                    let header = value()?;
                    if let Some((key, val)) = header.split_once(':') {
                        // `Name:` alone tells cURL to drop the header
                        if !val.trim().is_empty() {
                            headers.push((key.trim().to_string(), val.trim().to_string()));
                        }
                    } else if let Some(key) = header.strip_suffix(';') {
                        // `Name;` sends it empty
                        headers.push((key.trim().to_string(), String::new()));
                    }
                }
                "-d" | "--data" | "--data-ascii" | "--data-binary" => {
                    let v = value()?;
                    data.push(match v.strip_prefix('@') {
                        Some(path) => DataArg::File(path.to_string()),
                        None => DataArg::Raw(v),
                    });
                }
                "--data-raw" => data.push(DataArg::Raw(value()?)),
                "--data-urlencode" => data.push(DataArg::UrlEncode(value()?)),
                "--json" => {
                    json = true;
                    data.push(DataArg::Raw(value()?));
                }
                "-F" | "--form" => form.push(form_row(&value()?, false)),
                "--form-string" => form.push(form_row(&value()?, true)),
                "-T" | "--upload-file" => upload_file = Some(value()?),
                "-u" | "--user" => user = Some(value()?),
                "--oauth2-bearer" => bearer = Some(value()?),
                "-b" | "--cookie" => {
                    let v = value()?;
                    // without `=` it's a cookie file to read, which we can't
                    if v.contains('=') {
                        cookies.push(v);
                    }
                }
                "-A" | "--user-agent" => headers.push(("User-Agent".to_string(), value()?)),
                "-e" | "--referer" => headers.push(("Referer".to_string(), value()?)),
                "-I" | "--head" => head = true,
                "-G" | "--get" => get = true,
                "--url" => {
                    let v = value()?;
                    url.get_or_insert(v);
                }
                o if SKIPPED_WITH_VALUE.contains(&o) => {
                    value()?;
                }
                o if o.starts_with('-') && o.len() > 1 => {} // a flag we don't model
                _ => {
                    url.get_or_insert(option);
                }
            }
        }
    }

    let mut url = url.ok_or_else(|| "No URL found in the cURL command".to_string())?;
    if !url.contains("://") && !url.starts_with("{{") {
        url = format!("http://{url}");
    }

    let has_header = |headers: &[(String, String)], name: &str| {
        headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
    };

    if json {
        if !has_header(&headers, "Content-Type") {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        if !has_header(&headers, "Accept") {
            headers.push(("Accept".to_string(), "application/json".to_string()));
        }
    }
    if !cookies.is_empty() && !has_header(&headers, "Cookie") {
        headers.push(("Cookie".to_string(), cookies.join("; ")));
    }

    let mut body = None;
    if get && !data.is_empty() {
        // `-G` sends the data as the query string instead
        let query = join_data(&data);
        let sep = if url.contains('?') { '&' } else { '?' };
        url = format!("{url}{sep}{query}");
    } else if !form.is_empty() {
        body = Some(PostmanBody {
            mode: Some("formdata".to_string()),
            raw: None,
            formdata: Some(form),
            urlencoded: None,
            graphql: None,
            file: None,
        });
    } else if let [DataArg::File(path)] = data.as_slice() {
        body = Some(binary_body(path.clone()));
    } else if !data.is_empty() {
        if !json && !has_header(&headers, "Content-Type") {
            headers.push((
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ));
        }
        let urlencoded = urlencoded_rows(&data);
        body = Some(PostmanBody {
            mode: Some(
                if urlencoded.is_some() {
                    "urlencoded"
                } else {
                    "raw"
                }
                .to_string(),
            ),
            raw: urlencoded.is_none().then(|| join_data(&data)),
            formdata: None,
            urlencoded,
            graphql: None,
            file: None,
        });
    } else if let Some(path) = upload_file.clone() {
        body = Some(binary_body(path));
    }

    let method = method.unwrap_or_else(|| {
        if head {
            "HEAD"
        } else if upload_file.is_some() {
            "PUT"
        } else if body.is_some() {
            "POST"
        } else {
            "GET"
        }
        .to_string()
    });

    let auth = if let Some(user) = user {
        let (username, password) = user.split_once(':').unwrap_or((&user, ""));
        Some(RequestAuth {
            auth_type: AuthType::Basic,
            basic_username: username.to_string(),
            basic_password: password.to_string(),
            ..Default::default()
        })
    } else {
        bearer.map(|token| RequestAuth {
            auth_type: AuthType::Bearer,
            bearer_token: token,
            ..Default::default()
        })
    };

    Ok(PostmanRequestDetails {
        method,
        url: Some(PostmanUrl::String(url)),
        header: Some(
            headers
                .into_iter()
                .map(|(key, value)| PostmanHeader {
                    key,
                    value,
                    disabled: None,
                })
                .collect(),
        ),
        body,
        auth,
        description: None,
    })
}

/// a short tab name for the request: the last path segment, or the host.
pub fn suggested_name(request: &PostmanRequestDetails) -> String {
    let url = request
        .url
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();

    let without_scheme = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
    let path = without_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(without_scheme);

    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("Imported Request")
        .to_string()
}

fn is_curl(token: &str) -> bool {
    let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
    name.eq_ignore_ascii_case("curl") || name.eq_ignore_ascii_case("curl.exe")
}

fn binary_body(path: String) -> PostmanBody {
    PostmanBody {
        mode: Some("file".to_string()),
        raw: None,
        formdata: None,
        urlencoded: None,
        graphql: None,
        file: Some(PostmanBodyFile { src: Some(path) }),
    }
}

/// the body exactly as cURL would send it: arguments joined with `&`.
fn join_data(data: &[DataArg]) -> String {
    data.iter()
        .map(|d| match d {
            DataArg::Raw(s) => s.clone(),
            DataArg::File(path) => format!("@{path}"),
            DataArg::UrlEncode(s) => match s.split_once('=') {
                Some((name, value)) if !name.is_empty() => format!("{name}={}", encode(value)),
                Some((_, value)) => encode(value),
                None => encode(s),
            },
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// key/value rows when every argument is a `name=value` pair, so the body
/// opens as editable form fields instead of one encoded string.
fn urlencoded_rows(data: &[DataArg]) -> Option<Vec<PostmanBodyRow>> {
    data.iter()
        .map(|d| {
            let (key, value) = match d {
                DataArg::UrlEncode(s) => {
                    let (k, v) = s.split_once('=')?;
                    (k.to_string(), v.to_string())
                }
                DataArg::Raw(s) => {
                    // a raw `-d` is already encoded; only split simple `a=b` forms
                    if s.contains('&') || s.trim_start().starts_with(['{', '[', '<']) {
                        return None;
                    }
                    let (k, v) = s.split_once('=')?;
                    (decode(k)?, decode(v)?)
                }
                DataArg::File(_) => return None,
            };
            (!key.is_empty()).then(|| PostmanBodyRow {
                key,
                value: Some(value),
                disabled: None,
                r#type: Some("text".to_string()),
                content_type: None,
                src: None,
            })
        })
        .collect()
}

fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn decode(s: &str) -> Option<String> {
    let s = s.replace('+', " ");
    percent_encoding::percent_decode_str(&s)
        .decode_utf8()
        .ok()
        .map(|c| c.into_owned())
}

/// `name=value`, `name=@path`, `name=<path`, each optionally followed by
/// `;type=...` / `;filename=...`.
fn form_row(arg: &str, literal: bool) -> PostmanBodyRow {
    let (key, rest) = arg.split_once('=').unwrap_or((arg, ""));
    let mut row = PostmanBodyRow {
        key: key.to_string(),
        value: None,
        disabled: None,
        r#type: Some("text".to_string()),
        content_type: None,
        src: None,
    };
    if literal {
        row.value = Some(rest.to_string());
        return row;
    }

    let is_file = rest.starts_with('@') || rest.starts_with('<');
    let rest = if is_file { &rest[1..] } else { rest };
    let (value, params) = split_form_params(rest);
    for param in params {
        if let Some(t) = param.strip_prefix("type=") {
            row.content_type = Some(t.to_string());
        }
    }
    if is_file {
        row.r#type = Some("file".to_string());
        row.src = Some(PostmanFileSrc::One(value));
    } else {
        row.value = Some(value);
    }
    row
}

/// splits `value;type=x;filename=y` into the value and its parameters,
/// leaving a double-quoted value (which may contain `;`) intact.
fn split_form_params(s: &str) -> (String, Vec<String>) {
    let (value, rest) = if let Some(quoted) = s.strip_prefix('"') {
        match quoted.find('"') {
            Some(end) => (quoted[..end].to_string(), &quoted[end + 1..]),
            None => (quoted.to_string(), ""),
        }
    } else {
        match s.find(';') {
            Some(i) => (s[..i].to_string(), &s[i..]),
            None => (s.to_string(), ""),
        }
    };
    let params = rest
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    (value, params)
}

/// splits a command line into words, cmd-style when it uses `^` escapes
/// (Chrome's "Copy as cURL (cmd)"), POSIX-style otherwise.
fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let input = input.trim();
    let cmd_style = input.contains("^\"") || input.contains("^\n") || input.contains("^\r\n");
    if cmd_style {
        Ok(tokenize_cmd(input))
    } else {
        tokenize_posix(input)
    }
}

fn tokenize_posix(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    tokens.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\\' => match chars.next() {
                // line continuation
                Some('\n') => {}
                Some('\r') if chars.peek() == Some(&'\n') => {
                    chars.next();
                }
                Some(next) => {
                    current.push(next);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => current.push(ch),
                        None => return Err("Unterminated single quote".to_string()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(ch @ ('"' | '\\' | '$' | '`')) => current.push(ch),
                            Some('\n') => {}
                            Some(ch) => {
                                current.push('\\');
                                current.push(ch);
                            }
                            None => return Err("Unterminated double quote".to_string()),
                        },
                        Some(ch) => current.push(ch),
                        None => return Err("Unterminated double quote".to_string()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                read_ansi_c(&mut chars, &mut current)?;
            }
            c => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        tokens.push(current);
    }
    Ok(tokens)
}

/// the body of a bash `$'...'` string, after the opening quote.
fn read_ansi_c(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> Result<(), String> {
    loop {
        match chars.next() {
            Some('\'') => return Ok(()),
            Some('\\') => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some('e') | Some('E') => out.push('\x1b'),
                Some('x') => {
                    let hex: String = take_hex(chars, 2);
                    push_code(out, &hex);
                }
                Some('u') => {
                    let hex: String = take_hex(chars, 4);
                    push_code(out, &hex);
                }
                Some('U') => {
                    let hex: String = take_hex(chars, 8);
                    push_code(out, &hex);
                }
                Some(ch) => out.push(ch), // `\\`, `\'`, `\"`
                None => return Err("Unterminated $'...' string".to_string()),
            },
            Some(ch) => out.push(ch),
            None => return Err("Unterminated $'...' string".to_string()),
        }
    }
}

fn take_hex(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, max: usize) -> String {
    let mut hex = String::new();
    while hex.len() < max {
        match chars.peek() {
            Some(c) if c.is_ascii_hexdigit() => hex.push(chars.next().unwrap()),
            _ => break,
        }
    }
    hex
}

fn push_code(out: &mut String, hex: &str) {
    if let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
        out.push(c);
    }
}

/// cmd first strips `^` escapes, then the program splits its command line
/// with the MSVCRT rules: `"` toggles quoting, `\"` is a literal quote.
fn tokenize_cmd(input: &str) -> Vec<String> {
    let mut line = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '^' => match chars.next() {
                Some('\r') if chars.peek() == Some(&'\n') => {
                    chars.next();
                }
                Some('\n') | Some('\r') | None => {}
                Some(next) => line.push(next),
            },
            c => line.push(c),
        }
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                in_word = true;
                if chars.peek() == Some(&'"') {
                    current.extend(std::iter::repeat_n('\\', count / 2));
                    if count % 2 == 1 {
                        chars.next();
                        current.push('"');
                    }
                } else {
                    current.extend(std::iter::repeat_n('\\', count));
                }
            }
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    tokens.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            c => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        tokens.push(current);
    }
    tokens
}
