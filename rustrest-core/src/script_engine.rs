use boa_engine::{
    Context, JsValue, NativeFunction, Source, js_string,
    object::{FunctionObjectBuilder, ObjectInitializer, builtins::JsFunction},
    property::Attribute,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub struct ScriptExecutionContext {
    pub variables: HashMap<String, String>,
    // pub request_headers: HashMap<String, String>,
    pub response_body: String,
    pub response_status: u16,
}

pub struct ScriptRunner;

impl ScriptRunner {
    /// Formats console.log/info/warn/error args the way a JS console does:
    /// each argument stringified and joined with a space.
    fn format_console_args(
        args: &[JsValue],
        ctx: &mut Context,
    ) -> Result<String, boa_engine::JsError> {
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            let s = arg.to_string(ctx)?;
            parts.push(s.to_std_string_escaped());
        }
        Ok(parts.join(" "))
    }

    fn attach_console(
        context: &mut Context,
        logs: Rc<RefCell<Vec<String>>>,
    ) -> boa_engine::JsObject {
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
    /// used for both `pm.setVariable` and `pm.setHeader`
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

    /// Creates a fresh `Context`, registers `console` and a `pm` object assembled from
    /// `build_pm_members`, evaluates `script`, and returns the captured console logs.
    /// Shared by `run_pre_request` and `run_post_response`, which differ only in which
    /// native functions they expose on `pm`.
    fn run_script(
        script: &str,
        error_prefix: &str,
        build_pm_members: impl FnOnce(&mut Context) -> Vec<(&'static str, JsFunction)>,
    ) -> Result<Rc<RefCell<Vec<String>>>, String> {
        let mut context = Context::default();
        let logs_rc = Rc::new(RefCell::new(Vec::new()));

        let pm_members = build_pm_members(&mut context);
        let mut pm_init = ObjectInitializer::new(&mut context);
        for (name, func) in pm_members {
            pm_init.property(js_string!(name), func, Attribute::all());
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
    ) -> Result<Vec<String>, String> {
        if script.trim().is_empty() {
            return Ok(Vec::new());
        }

        let vars_rc = Rc::new(RefCell::new(variables.clone()));
        let hdrs_rc = Rc::new(RefCell::new(headers.clone()));
        let vars_out = vars_rc.clone();
        let hdrs_out = hdrs_rc.clone();

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

            vec![
                ("getVariable", get_var),
                ("setVariable", set_var),
                ("setHeader", set_header),
            ]
        })?;

        *variables = vars_out.borrow().clone();
        *headers = hdrs_out.borrow().clone();
        Ok(logs_rc.borrow().clone())
    }

    pub fn run_post_response(
        script: &str,
        exec_ctx: &ScriptExecutionContext,
    ) -> Result<(HashMap<String, String>, Vec<String>), String> {
        if script.trim().is_empty() {
            return Ok((exec_ctx.variables.clone(), Vec::new()));
        }

        let vars_rc = Rc::new(RefCell::new(exec_ctx.variables.clone()));
        let vars_out = vars_rc.clone();
        let body = exec_ctx.response_body.clone();
        let status = exec_ctx.response_status;

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
                NativeFunction::from_closure(move |_, _, _| Ok(JsValue::from(status as f64)))
            })
            .length(0)
            .build();
            let set_var = Self::make_kv_setter(context, vars_rc.clone());

            vec![
                ("getResponseBody", get_body),
                ("getStatus", get_status),
                ("setVariable", set_var),
            ]
        })?;

        Ok((vars_out.borrow().clone(), logs_rc.borrow().clone()))
    }
}
