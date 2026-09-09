use crate::http::{HttpMethod, HttpResponse, RequestSpec, TestResult, send_request};
use boa_engine::{
    Context, JsError, JsNativeError, JsValue, NativeFunction, Source, js_string,
    object::{FunctionObjectBuilder, JsObject, ObjectInitializer, builtins::JsFunction},
    property::{Attribute, PropertyKey},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub struct ScriptExecutionContext {
    pub variables: HashMap<String, String>,
    pub globals: HashMap<String, String>,
    pub response_body: String,
    pub response_status: u16,
    pub response_headers: HashMap<String, String>,
}

pub struct ScriptRunner;

impl ScriptRunner {
    /// Renders a single console.log/test/expect argument the way a JS console
    /// would: objects/arrays are shown as JSON, everything else via JS ToString.
    fn describe_value(value: &JsValue, ctx: &mut Context) -> String {
        if value.is_object() {
            if let Ok(Some(json)) = value.to_json(ctx) {
                return serde_json::to_string(&json).unwrap_or_else(|_| "[object]".to_string());
            }
        }
        value
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "<unprintable value>".to_string())
    }

    /// Formats console.log/info/warn/error args the way a JS console does:
    /// each argument stringified (objects pretty-printed as JSON) and joined with a space.
    fn format_console_args(args: &[JsValue], ctx: &mut Context) -> Result<String, JsError> {
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            parts.push(Self::describe_value(arg, ctx));
        }
        Ok(parts.join(" "))
    }

    fn attach_console(context: &mut Context, logs: Rc<RefCell<Vec<String>>>) -> JsObject {
        let make_fn =
            |level: &'static str, logs: Rc<RefCell<Vec<String>>>, context: &mut Context| {
                FunctionObjectBuilder::new(context.realm(), unsafe {
                    NativeFunction::from_closure(move |_, args, ctx| {
                        let line = Self::format_console_args(args, ctx)?;
                        logs.borrow_mut().push(format!("[{}] {}", level, line));
                        Ok(JsValue::undefined())
                    })
                })
                .length(0)
                .build()
            };

        let log_fn = make_fn("log", logs.clone(), context);
        let info_fn = make_fn("info", logs.clone(), context);
        let warn_fn = make_fn("warn", logs.clone(), context);
        let error_fn = make_fn("error", logs.clone(), context);

        ObjectInitializer::new(context)
            .property(js_string!("log"), log_fn, Attribute::all())
            .property(js_string!("info"), info_fn, Attribute::all())
            .property(js_string!("warn"), warn_fn, Attribute::all())
            .property(js_string!("error"), error_fn, Attribute::all())
            .build()
    }

    /// Builds a native function that inserts its two string args into `target`,
    /// used for `pm.setVariable`, `pm.setHeader` and every `<scope>.set(...)`.
    fn make_kv_setter(
        context: &mut Context,
        target: Rc<RefCell<HashMap<String, String>>>,
    ) -> JsFunction {
        FunctionObjectBuilder::new(context.realm(), unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let key = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?;
                let val = args
                    .get(1)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?;
                target
                    .borrow_mut()
                    .insert(key.to_std_string_escaped(), val.to_std_string_escaped());
                Ok(JsValue::undefined())
            })
        })
        .length(2)
        .build()
    }

    /// Builds variable scope object (`pm.environment`, `pm.variables`, `pm.globals`) exposing `.get/.set/.has/.unset/.clear`, all backed by `store`.
    fn build_scope_object(
        context: &mut Context,
        store: Rc<RefCell<HashMap<String, String>>>,
    ) -> JsObject {
        let get_fn = {
            let store = store.clone();
            FunctionObjectBuilder::new(context.realm(), unsafe {
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args
                        .get(0)
                        .unwrap_or(&JsValue::undefined())
                        .to_string(ctx)?;
                    Ok(match store.borrow().get(&key.to_std_string_escaped()) {
                        Some(v) => JsValue::from(js_string!(v.clone())),
                        None => JsValue::undefined(),
                    })
                })
            })
            .length(1)
            .build()
        };
        let set_fn = Self::make_kv_setter(context, store.clone());
        let has_fn = {
            let store = store.clone();
            FunctionObjectBuilder::new(context.realm(), unsafe {
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args
                        .get(0)
                        .unwrap_or(&JsValue::undefined())
                        .to_string(ctx)?;
                    Ok(JsValue::from(
                        store.borrow().contains_key(&key.to_std_string_escaped()),
                    ))
                })
            })
            .length(1)
            .build()
        };
        let unset_fn = {
            let store = store.clone();
            FunctionObjectBuilder::new(context.realm(), unsafe {
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args
                        .get(0)
                        .unwrap_or(&JsValue::undefined())
                        .to_string(ctx)?;
                    store.borrow_mut().remove(&key.to_std_string_escaped());
                    Ok(JsValue::undefined())
                })
            })
            .length(1)
            .build()
        };
        let clear_fn = {
            let store = store.clone();
            FunctionObjectBuilder::new(context.realm(), unsafe {
                NativeFunction::from_closure(move |_, _, _| {
                    store.borrow_mut().clear();
                    Ok(JsValue::undefined())
                })
            })
            .length(0)
            .build()
        };

        ObjectInitializer::new(context)
            .property(js_string!("get"), get_fn, Attribute::all())
            .property(js_string!("set"), set_fn, Attribute::all())
            .property(js_string!("has"), has_fn, Attribute::all())
            .property(js_string!("unset"), unset_fn, Attribute::all())
            .property(js_string!("clear"), clear_fn, Attribute::all())
            .build()
    }

    fn parse_http_method(method: &str) -> HttpMethod {
        match method.trim().to_uppercase().as_str() {
            "GET" => HttpMethod::GET,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "PATCH" => HttpMethod::PATCH,
            "HEAD" => HttpMethod::HEAD,
            "OPTIONS" => HttpMethod::OPTIONS,
            other => HttpMethod::Custom(other.to_string()),
        }
    }

    fn parse_send_request_options(
        opt: &JsValue,
        ctx: &mut Context,
    ) -> Result<(String, HttpMethod, Vec<(String, String)>, String), String> {
        if let Some(s) = opt.as_string() {
            return Ok((
                s.to_std_string_escaped(),
                HttpMethod::GET,
                Vec::new(),
                String::new(),
            ));
        }
        let obj = opt.as_object().ok_or_else(|| {
            "pm.sendRequest: options must be a URL string or an options object".to_string()
        })?;

        let url_val = obj.get(js_string!("url"), ctx).map_err(|e| e.to_string())?;
        let url = if url_val.is_undefined() {
            String::new()
        } else {
            url_val
                .to_string(ctx)
                .map_err(|e| e.to_string())?
                .to_std_string_escaped()
        };

        let method_val = obj
            .get(js_string!("method"), ctx)
            .map_err(|e| e.to_string())?;
        let method = if method_val.is_undefined() {
            HttpMethod::GET
        } else {
            let s = method_val
                .to_string(ctx)
                .map_err(|e| e.to_string())?
                .to_std_string_escaped();
            Self::parse_http_method(&s)
        };

        let mut headers = Vec::new();
        for key_name in ["header", "headers"] {
            let h = obj
                .get(js_string!(key_name), ctx)
                .map_err(|e| e.to_string())?;
            let Some(hobj) = h.as_object() else { continue };
            if hobj.is_array() {
                let len = hobj
                    .get(js_string!("length"), ctx)
                    .map_err(|e| e.to_string())?
                    .to_number(ctx)
                    .map_err(|e| e.to_string())? as u32;
                for i in 0..len {
                    let item = hobj.get(i, ctx).map_err(|e| e.to_string())?;
                    let Some(item_obj) = item.as_object() else {
                        continue;
                    };
                    let k = item_obj
                        .get(js_string!("key"), ctx)
                        .map_err(|e| e.to_string())?;
                    if k.is_undefined() {
                        continue;
                    }
                    let k = k
                        .to_string(ctx)
                        .map_err(|e| e.to_string())?
                        .to_std_string_escaped();
                    let v = item_obj
                        .get(js_string!("value"), ctx)
                        .map_err(|e| e.to_string())?;
                    let v = if v.is_undefined() {
                        String::new()
                    } else {
                        v.to_string(ctx)
                            .map_err(|e| e.to_string())?
                            .to_std_string_escaped()
                    };
                    if !k.trim().is_empty() {
                        headers.push((k, v));
                    }
                }
            } else {
                let keys = hobj.own_property_keys(ctx).map_err(|e| e.to_string())?;
                for pk in keys {
                    if let PropertyKey::String(js_key) = &pk {
                        let v = hobj.get(pk.clone(), ctx).map_err(|e| e.to_string())?;
                        if !v.is_undefined() {
                            let v = v
                                .to_string(ctx)
                                .map_err(|e| e.to_string())?
                                .to_std_string_escaped();
                            headers.push((js_key.to_std_string_escaped(), v));
                        }
                    }
                }
            }
        }

        let body_val = obj
            .get(js_string!("body"), ctx)
            .map_err(|e| e.to_string())?;
        let body = if body_val.is_undefined() || body_val.is_null() {
            String::new()
        } else if let Some(s) = body_val.as_string() {
            s.to_std_string_escaped()
        } else if let Some(body_obj) = body_val.as_object() {
            let raw = body_obj
                .get(js_string!("raw"), ctx)
                .map_err(|e| e.to_string())?;
            if !raw.is_undefined() {
                raw.to_string(ctx)
                    .map_err(|e| e.to_string())?
                    .to_std_string_escaped()
            } else {
                match body_val.to_json(ctx) {
                    Ok(Some(json_val)) => serde_json::to_string(&json_val).unwrap_or_default(),
                    _ => String::new(),
                }
            }
        } else {
            String::new()
        };

        Ok((url, method, headers, body))
    }

    /// Runs `spec` to completion on a dedicated OS thread with its own tiny tokio
    /// runtime, blocking the calling thread until the response (or error) is ready.
    /// This keeps the boa script engine's fully-synchronous execution model intact -
    /// no job queue / microtask draining is needed, while still letting
    /// `pm.sendRequest`'s callback run synchronously before the script continues.
    fn blocking_send(spec: RequestSpec) -> Result<HttpResponse, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(send_request(
                    spec,
                    tokio_util::sync::CancellationToken::new(),
                )),
                Err(e) => Err(format!("Failed to start async runtime: {}", e)),
            };
            let _ = tx.send(outcome);
        });
        rx.recv().unwrap_or_else(|_| {
            Err("pm.sendRequest: worker thread terminated unexpectedly".to_string())
        })
    }

    /// Builds the response object passed to a `pm.sendRequest` callback, and reused
    /// for `pm.response` in post-response scripts: `.code`, `.status`, `.body`,
    /// `.headers` (with a `.get(name)` helper) plus `.json()` and `.text()`.
    fn build_response_object(
        ctx: &mut Context,
        body: &str,
        status: u16,
        headers: &HashMap<String, String>,
    ) -> JsObject {
        let body_owned = body.to_string();
        let headers_owned = headers.clone();

        let get_header_fn = {
            let headers_for_get = headers_owned.clone();
            FunctionObjectBuilder::new(ctx.realm(), unsafe {
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args
                        .get(0)
                        .unwrap_or(&JsValue::undefined())
                        .to_string(ctx)?
                        .to_std_string_escaped();
                    let found = headers_for_get
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(&key))
                        .map(|(_, v)| v.clone());
                    Ok(match found {
                        Some(v) => JsValue::from(js_string!(v)),
                        None => JsValue::undefined(),
                    })
                })
            })
            .length(1)
            .build()
        };

        let mut headers_init = ObjectInitializer::new(ctx);
        for (k, v) in &headers_owned {
            headers_init.property(
                js_string!(k.clone()),
                js_string!(v.clone()),
                Attribute::all(),
            );
        }
        headers_init.property(js_string!("get"), get_header_fn, Attribute::all());
        let headers_obj = headers_init.build();

        let status_text = reqwest::StatusCode::from_u16(status)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or("")
            .to_string();

        let json_fn =
            {
                let body_for_json = body_owned.clone();
                FunctionObjectBuilder::new(ctx.realm(), unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let parsed: serde_json::Value = serde_json::from_str(&body_for_json)
                            .map_err(|e| {
                                JsError::from(JsNativeError::syntax().with_message(format!(
                                    "Response body is not valid JSON: {}",
                                    e
                                )))
                            })?;
                        JsValue::from_json(&parsed, ctx)
                    })
                })
                .length(0)
                .build()
            };
        let text_fn = {
            let body_for_text = body_owned.clone();
            FunctionObjectBuilder::new(ctx.realm(), unsafe {
                NativeFunction::from_closure(move |_, _, _| {
                    Ok(JsValue::from(js_string!(body_for_text.clone())))
                })
            })
            .length(0)
            .build()
        };

        let to_obj = Self::build_response_assertion(
            ctx,
            body_owned.clone(),
            status,
            status_text.clone(),
            headers_owned.clone(),
            false,
        );

        ObjectInitializer::new(ctx)
            .property(js_string!("code"), status as i32, Attribute::all())
            .property(
                js_string!("status"),
                js_string!(status_text),
                Attribute::all(),
            )
            .property(
                js_string!("body"),
                js_string!(body_owned.clone()),
                Attribute::all(),
            )
            .property(js_string!("headers"), headers_obj, Attribute::all())
            .property(js_string!("json"), json_fn, Attribute::all())
            .property(js_string!("text"), text_fn, Attribute::all())
            .property(js_string!("to"), to_obj, Attribute::all())
            .build()
    }

    /// Implements `pm.response.to.*` sugar (a separate, smaller chain
    /// from `pm.expect`): `.to.have.status(code|text)`, `.to.have.header(name[, value])`,
    /// `.to.have.body([substring])`, and the `.to.be.ok` getter (2xx status).
    fn build_response_assertion(
        ctx: &mut Context,
        body: String,
        status: u16,
        status_text: String,
        headers: HashMap<String, String>,
        negated: bool,
    ) -> JsObject {
        let status_fn = {
            let status_text = status_text.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let condition = if let Some(n) = expected.as_number() {
                    status as f64 == n
                } else {
                    expected
                        .as_string()
                        .map(|s| s.to_std_string_escaped() == status_text)
                        .unwrap_or(false)
                };
                let e_desc = Self::describe_value(&expected, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!(
                        "expected response to have status {}, got {} {}",
                        e_desc, status, status_text
                    ),
                    format!("expected response to not have status {}", e_desc),
                )
            })
        };
        let header_fn = {
            let headers = headers.clone();
            Self::assertion_method(ctx, 2, move |args, ctx| {
                let name = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let found = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name));
                let condition = match args.get(1) {
                    Some(expected_val) => {
                        let expected_str = expected_val.to_string(ctx)?.to_std_string_escaped();
                        found.map(|(_, v)| v == &expected_str).unwrap_or(false)
                    }
                    None => found.is_some(),
                };
                Self::assert(
                    negated,
                    condition,
                    format!("expected response to have header `{}`", name),
                    format!("expected response to not have header `{}`", name),
                )
            })
        };
        let body_fn = {
            let body = body.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let condition = match args.get(0) {
                    Some(expected) => {
                        let expected_str = expected.to_string(ctx)?.to_std_string_escaped();
                        body.contains(&expected_str)
                    }
                    None => !body.is_empty(),
                };
                Self::assert(
                    negated,
                    condition,
                    "expected response to have a matching body".to_string(),
                    "expected response to not have a matching body".to_string(),
                )
            })
        };
        let ok_get = Self::assertion_method(ctx, 0, move |_, _| {
            let condition = (200..300).contains(&status);
            Self::assert(
                negated,
                condition,
                format!("expected response to be ok, got status {}", status),
                format!("expected response to not be ok, got status {}", status),
            )
        });

        let not_obj = (!negated).then(|| {
            Self::build_response_assertion(
                ctx,
                body.clone(),
                status,
                status_text.clone(),
                headers.clone(),
                true,
            )
        });

        let mut init = ObjectInitializer::new(ctx);
        init.property(js_string!("status"), status_fn, Attribute::all());
        init.property(js_string!("header"), header_fn, Attribute::all());
        init.property(js_string!("body"), body_fn, Attribute::all());
        init.accessor(js_string!("ok"), Some(ok_get), None, Attribute::all());
        if let Some(not_obj) = not_obj {
            init.property(js_string!("not"), not_obj, Attribute::all());
        }

        let obj = init.build();
        for chain_word in ["to", "have", "be", "and", "with"] {
            let _ = obj.create_data_property(js_string!(chain_word), obj.clone(), ctx);
        }
        obj
    }

    /// Implements `pm.sendRequest(urlOrOptions, callback)`. Runs the request to
    /// completion and then invokes `callback(err, res)` synchronously
    fn attach_send_request(context: &mut Context) -> JsFunction {
        FunctionObjectBuilder::new(context.realm(), unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let opt = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let callback = args
                    .get(1)
                    .and_then(|v| v.as_object())
                    .and_then(JsFunction::from_object);

                let parsed = Self::parse_send_request_options(&opt, ctx);
                let (url, method, headers, body) = match parsed {
                    Ok(v) => v,
                    Err(e) => {
                        return match &callback {
                            Some(cb) => cb.call(
                                &JsValue::undefined(),
                                &[JsValue::from(js_string!(e)), JsValue::null()],
                                ctx,
                            ),
                            None => Err(JsNativeError::typ().with_message(e).into()),
                        };
                    }
                };

                if url.trim().is_empty() {
                    let msg = "pm.sendRequest: 'url' is required".to_string();
                    return match &callback {
                        Some(cb) => cb.call(
                            &JsValue::undefined(),
                            &[JsValue::from(js_string!(msg)), JsValue::null()],
                            ctx,
                        ),
                        None => Err(JsNativeError::typ().with_message(msg).into()),
                    };
                }

                let spec = RequestSpec::new(url, method)
                    .headers(headers)
                    .body_type(if body.trim().is_empty() {
                        crate::common::BodyType::None
                    } else {
                        crate::common::BodyType::Raw
                    })
                    .raw_body(body);

                let result = Self::blocking_send(spec);

                let (err_val, res_val) = match result {
                    Ok(resp) => (
                        JsValue::null(),
                        JsValue::from(Self::build_response_object(
                            ctx,
                            &resp.body,
                            resp.status,
                            &resp.headers,
                        )),
                    ),
                    Err(e) => (JsValue::from(js_string!(e)), JsValue::null()),
                };

                match &callback {
                    Some(cb) => cb.call(&JsValue::undefined(), &[err_val, res_val], ctx),
                    None => Ok(JsValue::undefined()),
                }
            })
        })
        .length(2)
        .build()
    }

    /// Implements `pm.test(name, fn)`: runs `fn` and records a pass/fail `TestResult`
    fn attach_test(context: &mut Context, results: Rc<RefCell<Vec<TestResult>>>) -> JsFunction {
        FunctionObjectBuilder::new(context.realm(), unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let name = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let callback = args
                    .get(1)
                    .and_then(|v| v.as_object())
                    .and_then(JsFunction::from_object);
                let passed = match &callback {
                    Some(cb) => cb.call(&JsValue::undefined(), &[], ctx).is_ok(),
                    None => true,
                };
                results.borrow_mut().push(TestResult { name, passed });
                Ok(JsValue::undefined())
            })
        })
        .length(2)
        .build()
    }

    fn values_equal(a: &JsValue, b: &JsValue, ctx: &mut Context) -> bool {
        if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
            return a_num == b_num;
        }
        a.to_json(ctx).ok().flatten() == b.to_json(ctx).ok().flatten()
    }

    /// Evaluates a single chai-style assertion: throws a `TypeError` (which
    /// `pm.test` catches to mark the test failed) when the check doesn't hold.
    fn assert(
        negated: bool,
        condition: bool,
        msg_positive: String,
        msg_negative: String,
    ) -> Result<JsValue, JsError> {
        let ok = if negated { !condition } else { condition };
        if ok {
            Ok(JsValue::undefined())
        } else {
            Err(JsNativeError::typ()
                .with_message(if negated { msg_negative } else { msg_positive })
                .into())
        }
    }

    fn assertion_method(
        ctx: &mut Context,
        len: usize,
        f: impl Fn(&[JsValue], &mut Context) -> Result<JsValue, JsError> + 'static,
    ) -> JsFunction {
        FunctionObjectBuilder::new(ctx.realm(), unsafe {
            NativeFunction::from_closure(move |_, args, ctx| f(args, ctx))
        })
        .length(len)
        .build()
    }

    fn build_assertion(ctx: &mut Context, actual: JsValue, negated: bool) -> JsObject {
        let type_of_value = |v: &JsValue| -> String {
            if v.is_null() {
                "null".to_string()
            } else if v.as_object().map(|o| o.is_array()).unwrap_or(false) {
                "array".to_string()
            } else {
                v.type_of().to_string()
            }
        };

        let equal_fn = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let condition = Self::values_equal(&actual, &expected, ctx);
                let (a_desc, e_desc) = (
                    Self::describe_value(&actual, ctx),
                    Self::describe_value(&expected, ctx),
                );
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to equal {}", a_desc, e_desc),
                    format!("expected {} to not equal {}", a_desc, e_desc),
                )
            })
        };
        let eql_fn = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let condition = Self::values_equal(&actual, &expected, ctx);
                let (a_desc, e_desc) = (
                    Self::describe_value(&actual, ctx),
                    Self::describe_value(&expected, ctx),
                );
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to deeply equal {}", a_desc, e_desc),
                    format!("expected {} to not deeply equal {}", a_desc, e_desc),
                )
            })
        };
        let a_fn = {
            let actual = actual.clone();
            let type_of_value = type_of_value.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected_type = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let actual_type = type_of_value(&actual);
                let condition = actual_type == expected_type;
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!(
                        "expected {} to be a `{}`, got `{}`",
                        a_desc, expected_type, actual_type
                    ),
                    format!("expected {} not to be a `{}`", a_desc, expected_type),
                )
            })
        };
        let an_fn = {
            let actual = actual.clone();
            let type_of_value = type_of_value.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected_type = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let actual_type = type_of_value(&actual);
                let condition = actual_type == expected_type;
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!(
                        "expected {} to be an `{}`, got `{}`",
                        a_desc, expected_type, actual_type
                    ),
                    format!("expected {} not to be an `{}`", a_desc, expected_type),
                )
            })
        };
        let include_fn = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let needle = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let condition = if let Some(s) = actual.as_string() {
                    needle
                        .as_string()
                        .map(|n| {
                            s.to_std_string_escaped()
                                .contains(&n.to_std_string_escaped())
                        })
                        .unwrap_or(false)
                } else if let Some(obj) = actual.as_object() {
                    if obj.is_array() {
                        let len = obj
                            .get(js_string!("length"), ctx)
                            .and_then(|v| v.to_number(ctx))
                            .unwrap_or(0.0) as u32;
                        let mut found = false;
                        for i in 0..len {
                            if let Ok(item) = obj.get(i, ctx) {
                                if Self::values_equal(&item, &needle, ctx) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                        found
                    } else {
                        false
                    }
                } else {
                    false
                };
                let (a_desc, n_desc) = (
                    Self::describe_value(&actual, ctx),
                    Self::describe_value(&needle, ctx),
                );
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to include {}", a_desc, n_desc),
                    format!("expected {} to not include {}", a_desc, n_desc),
                )
            })
        };
        let property_fn = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let name = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let condition = actual
                    .as_object()
                    .map(|o| {
                        o.has_property(js_string!(name.clone()), ctx)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to have property `{}`", a_desc, name),
                    format!("expected {} to not have property `{}`", a_desc, name),
                )
            })
        };
        let length_of_fn = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected_len = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_number(ctx)?;
                let actual_len = if let Some(s) = actual.as_string() {
                    s.len() as f64
                } else if let Some(obj) = actual.as_object() {
                    obj.get(js_string!("length"), ctx)
                        .and_then(|v| v.to_number(ctx))
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                let condition = actual_len == expected_len;
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!(
                        "expected {} to have length {}, got {}",
                        a_desc, expected_len, actual_len
                    ),
                    format!("expected {} to not have length {}", a_desc, expected_len),
                )
            })
        };

        let numeric_fn = |ctx: &mut Context, op: fn(f64, f64) -> bool, verb: &'static str| {
            let actual = actual.clone();
            Self::assertion_method(ctx, 1, move |args, ctx| {
                let expected = args
                    .get(0)
                    .unwrap_or(&JsValue::undefined())
                    .to_number(ctx)?;
                let actual_num = actual.to_number(ctx).unwrap_or(f64::NAN);
                let condition = op(actual_num, expected);
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be {} {}", a_desc, verb, expected),
                    format!("expected {} to not be {} {}", a_desc, verb, expected),
                )
            })
        };
        let above_fn = numeric_fn(ctx, |a, b| a > b, "above");
        let below_fn = numeric_fn(ctx, |a, b| a < b, "below");
        let least_fn = numeric_fn(ctx, |a, b| a >= b, "least");
        let most_fn = numeric_fn(ctx, |a, b| a <= b, "most");

        let bool_getter = |ctx: &mut Context, name: &'static str, expected: Option<bool>| {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = actual.as_boolean() == expected;
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be {}", a_desc, name),
                    format!("expected {} to not be {}", a_desc, name),
                )
            })
        };
        let true_get = bool_getter(ctx, "true", Some(true));
        let false_get = bool_getter(ctx, "false", Some(false));

        let null_get = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = actual.is_null();
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be null", a_desc),
                    format!("expected {} to not be null", a_desc),
                )
            })
        };
        let undefined_get = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = actual.is_undefined();
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be undefined", a_desc),
                    format!("expected {} to not be undefined", a_desc),
                )
            })
        };
        let exist_get = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = !actual.is_null() && !actual.is_undefined();
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to exist", a_desc),
                    format!("expected {} to not exist", a_desc),
                )
            })
        };
        let ok_get = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = actual.to_boolean();
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be truthy", a_desc),
                    format!("expected {} to not be truthy", a_desc),
                )
            })
        };
        let empty_get = {
            let actual = actual.clone();
            Self::assertion_method(ctx, 0, move |_, ctx| {
                let condition = if let Some(s) = actual.as_string() {
                    s.is_empty()
                } else if let Some(obj) = actual.as_object() {
                    obj.get(js_string!("length"), ctx)
                        .and_then(|v| v.to_number(ctx))
                        .unwrap_or(-1.0)
                        == 0.0
                } else {
                    false
                };
                let a_desc = Self::describe_value(&actual, ctx);
                Self::assert(
                    negated,
                    condition,
                    format!("expected {} to be empty", a_desc),
                    format!("expected {} to not be empty", a_desc),
                )
            })
        };

        // built before `ObjectInitializer` takes ownership of `ctx`'s borrow, since
        // it needs its own use of `ctx` and doesn't reference the parent object
        let not_obj = (!negated).then(|| Self::build_assertion(ctx, actual.clone(), true));

        let mut init = ObjectInitializer::new(ctx);
        init.property(js_string!("equal"), equal_fn, Attribute::all());
        init.property(js_string!("eql"), eql_fn, Attribute::all());
        init.property(js_string!("a"), a_fn, Attribute::all());
        init.property(js_string!("an"), an_fn, Attribute::all());
        init.property(js_string!("include"), include_fn.clone(), Attribute::all());
        init.property(js_string!("contain"), include_fn, Attribute::all());
        init.property(js_string!("property"), property_fn, Attribute::all());
        init.property(js_string!("lengthOf"), length_of_fn, Attribute::all());
        init.property(js_string!("above"), above_fn, Attribute::all());
        init.property(js_string!("below"), below_fn, Attribute::all());
        init.property(js_string!("least"), least_fn, Attribute::all());
        init.property(js_string!("most"), most_fn, Attribute::all());
        init.accessor(js_string!("true"), Some(true_get), None, Attribute::all());
        init.accessor(js_string!("false"), Some(false_get), None, Attribute::all());
        init.accessor(js_string!("null"), Some(null_get), None, Attribute::all());
        init.accessor(
            js_string!("undefined"),
            Some(undefined_get),
            None,
            Attribute::all(),
        );
        init.accessor(js_string!("exist"), Some(exist_get), None, Attribute::all());
        init.accessor(js_string!("ok"), Some(ok_get), None, Attribute::all());
        init.accessor(js_string!("empty"), Some(empty_get), None, Attribute::all());
        if let Some(not_obj) = not_obj {
            init.property(js_string!("not"), not_obj, Attribute::all());
        }

        let obj = init.build();

        // self-referential chain words (`.to.be.have...`) all resolve back to `obj`
        for chain_word in [
            "to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of",
            "same", "but", "does",
        ] {
            let _ = obj.create_data_property(js_string!(chain_word), obj.clone(), ctx);
        }
        obj
    }

    fn attach_expect(context: &mut Context) -> JsFunction {
        FunctionObjectBuilder::new(context.realm(), unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let actual = args.get(0).cloned().unwrap_or(JsValue::undefined());
                Ok(JsValue::from(Self::build_assertion(ctx, actual, false)))
            })
        })
        .length(1)
        .build()
    }

    /// Creates a fresh `Context`, registers `console` and a `pm` object assembled from
    /// `build_pm_members`, evaluates `script`, and returns the captured console logs.
    /// Shared by `run_pre_request` and `run_post_response`, which differ only in which
    /// members they expose on `pm`.
    fn run_script(
        script: &str,
        error_prefix: &str,
        build_pm_members: impl FnOnce(&mut Context) -> Vec<(&'static str, JsValue)>,
    ) -> Result<Rc<RefCell<Vec<String>>>, String> {
        let mut context = Context::default();
        let logs_rc = Rc::new(RefCell::new(Vec::new()));

        let pm_members = build_pm_members(&mut context);
        let mut pm_init = ObjectInitializer::new(&mut context);
        for (name, value) in pm_members {
            pm_init.property(js_string!(name), value, Attribute::all());
        }
        let pm_obj = pm_init.build();
        context
            .register_global_property(js_string!("pm"), pm_obj, Attribute::all())
            .map_err(|e| e.to_string())?;

        let console_obj = Self::attach_console(&mut context, logs_rc.clone());
        context
            .register_global_property(js_string!("console"), console_obj, Attribute::all())
            .map_err(|e| e.to_string())?;

        context
            .eval(Source::from_bytes(script.as_bytes()))
            .map_err(|e| format!("{}: {}", error_prefix, e))?;

        Ok(logs_rc)
    }

    pub fn run_pre_request(
        script: &str,
        variables: &mut HashMap<String, String>,
        headers: &mut HashMap<String, String>,
        globals: &mut HashMap<String, String>,
    ) -> Result<Vec<String>, String> {
        if script.trim().is_empty() {
            return Ok(Vec::new());
        }

        let vars_rc = Rc::new(RefCell::new(variables.clone()));
        let hdrs_rc = Rc::new(RefCell::new(headers.clone()));
        let globals_rc = Rc::new(RefCell::new(globals.clone()));
        let vars_out = vars_rc.clone();
        let hdrs_out = hdrs_rc.clone();
        let globals_out = globals_rc.clone();

        let logs_rc = Self::run_script(script, "Pre-request Script Error", move |context| {
            let get_var = {
                let vars = vars_rc.clone();
                FunctionObjectBuilder::new(context.realm(), unsafe {
                    NativeFunction::from_closure(move |_, args, ctx| {
                        let key = args
                            .get(0)
                            .unwrap_or(&JsValue::undefined())
                            .to_string(ctx)?;
                        let val = vars
                            .borrow()
                            .get(&key.to_std_string_escaped())
                            .cloned()
                            .unwrap_or_default();
                        Ok(JsValue::from(js_string!(val)))
                    })
                })
                .length(1)
                .build()
            };
            let set_var = Self::make_kv_setter(context, vars_rc.clone());
            let set_header = Self::make_kv_setter(context, hdrs_rc.clone());
            let environment_obj = Self::build_scope_object(context, vars_rc.clone());
            let variables_obj = Self::build_scope_object(context, vars_rc.clone());
            let globals_obj = Self::build_scope_object(context, globals_rc.clone());
            let send_request_fn = Self::attach_send_request(context);

            vec![
                ("getVariable", get_var.into()),
                ("setVariable", set_var.into()),
                ("setHeader", set_header.into()),
                ("environment", environment_obj.into()),
                ("variables", variables_obj.into()),
                ("globals", globals_obj.into()),
                ("sendRequest", send_request_fn.into()),
            ]
        })?;

        *variables = vars_out.borrow().clone();
        *headers = hdrs_out.borrow().clone();
        *globals = globals_out.borrow().clone();
        Ok(logs_rc.borrow().clone())
    }

    pub fn run_post_response(
        script: &str,
        exec_ctx: &ScriptExecutionContext,
    ) -> Result<
        (
            HashMap<String, String>,
            HashMap<String, String>,
            Vec<TestResult>,
            Vec<String>,
        ),
        String,
    > {
        if script.trim().is_empty() {
            return Ok((
                exec_ctx.variables.clone(),
                exec_ctx.globals.clone(),
                Vec::new(),
                Vec::new(),
            ));
        }

        let vars_rc = Rc::new(RefCell::new(exec_ctx.variables.clone()));
        let globals_rc = Rc::new(RefCell::new(exec_ctx.globals.clone()));
        let vars_out = vars_rc.clone();
        let globals_out = globals_rc.clone();
        let body = exec_ctx.response_body.clone();
        let status = exec_ctx.response_status;
        let headers = exec_ctx.response_headers.clone();
        let test_results_rc: Rc<RefCell<Vec<TestResult>>> = Rc::new(RefCell::new(Vec::new()));
        let test_results_out = test_results_rc.clone();

        let logs_rc = Self::run_script(script, "Post-response Script Error", move |context| {
            let get_body = {
                let body = body.clone();
                FunctionObjectBuilder::new(context.realm(), unsafe {
                    NativeFunction::from_closure(move |_, _, _| {
                        Ok(JsValue::from(js_string!(body.clone())))
                    })
                })
                .length(0)
                .build()
            };
            let get_status = FunctionObjectBuilder::new(context.realm(), unsafe {
                NativeFunction::from_closure(move |_, _, _| Ok(JsValue::from(status as i32)))
            })
            .length(0)
            .build();
            let get_var = {
                let vars = vars_rc.clone();
                FunctionObjectBuilder::new(context.realm(), unsafe {
                    NativeFunction::from_closure(move |_, args, ctx| {
                        let key = args
                            .get(0)
                            .unwrap_or(&JsValue::undefined())
                            .to_string(ctx)?;
                        Ok(match vars.borrow().get(&key.to_std_string_escaped()) {
                            Some(v) => JsValue::from(js_string!(v.clone())),
                            None => JsValue::undefined(),
                        })
                    })
                })
                .length(1)
                .build()
            };
            let set_var = Self::make_kv_setter(context, vars_rc.clone());
            let environment_obj = Self::build_scope_object(context, vars_rc.clone());
            let variables_obj = Self::build_scope_object(context, vars_rc.clone());
            let globals_obj = Self::build_scope_object(context, globals_rc.clone());
            let response_obj = Self::build_response_object(context, &body, status, &headers);
            let send_request_fn = Self::attach_send_request(context);
            let test_fn = Self::attach_test(context, test_results_rc.clone());
            let expect_fn = Self::attach_expect(context);

            vec![
                ("getResponseBody", get_body.into()),
                ("getStatus", get_status.into()),
                ("getVariable", get_var.into()),
                ("setVariable", set_var.into()),
                ("environment", environment_obj.into()),
                ("variables", variables_obj.into()),
                ("globals", globals_obj.into()),
                ("response", JsValue::from(response_obj)),
                ("sendRequest", send_request_fn.into()),
                ("test", test_fn.into()),
                ("expect", expect_fn.into()),
            ]
        })?;

        Ok((
            vars_out.borrow().clone(),
            globals_out.borrow().clone(),
            test_results_out.borrow().clone(),
            logs_rc.borrow().clone(),
        ))
    }
}
