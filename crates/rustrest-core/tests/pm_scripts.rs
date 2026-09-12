use rustrest_core::script_engine::{ScriptExecutionContext, ScriptRunner};
use std::collections::HashMap;

#[test]
fn pm_response_to_chain_matches_chai_postman_style_assertions() {
    let script = r#"
        pm.test("status via pm.response.to.have.status(200)", function () {
            pm.response.to.have.status(200);
        });
        pm.test("status text via pm.response.to.have.status('OK')", function () {
            pm.response.to.have.status("OK");
        });
        pm.test("body does not include 'error'", function () {
            pm.response.to.not.have.body("error");
        });
        pm.test("response is ok", function () {
            pm.response.to.be.ok;
        });
        pm.test("wrong status should fail", function () {
            pm.response.to.have.status(404);
        });
    "#;

    let exec_ctx = ScriptExecutionContext {
        variables: HashMap::new(),
        globals: HashMap::new(),
        response_body: r#"{"data": [1, 2, 3]}"#.to_string(),
        response_status: 200,
        response_headers: HashMap::new(),
    };

    let (_vars, _globals, tests, _logs) = ScriptRunner::run_post_response(script, &exec_ctx)
        .expect("script should run without error");

    assert_eq!(tests.len(), 5);
    assert!(tests[0].passed, "{:?}", tests[0]);
    assert!(tests[1].passed, "{:?}", tests[1]);
    assert!(tests[2].passed, "{:?}", tests[2]);
    assert!(tests[3].passed, "{:?}", tests[3]);
    assert!(!tests[4].passed, "{:?}", tests[4]);
}

#[test]
fn pm_test_and_expect_record_pass_and_fail_results() {
    let script = r#"
        pm.test("status is 200", function () {
            pm.expect(pm.response.code).to.equal(200);
        });
        pm.test("body has data", function () {
            pm.expect(pm.response.json()).to.have.property("data");
        });
        pm.test("this should fail", function () {
            pm.expect(1).to.equal(2);
        });
    "#;

    let exec_ctx = ScriptExecutionContext {
        variables: HashMap::new(),
        globals: HashMap::new(),
        response_body: r#"{"data": {}}"#.to_string(),
        response_status: 200,
        response_headers: HashMap::new(),
    };

    let (_vars, _globals, tests, _logs) = ScriptRunner::run_post_response(script, &exec_ctx)
        .expect("script should run without error");

    assert_eq!(tests.len(), 3);
    assert!(tests[0].passed, "{:?}", tests[0]);
    assert!(tests[1].passed, "{:?}", tests[1]);
    assert!(!tests[2].passed, "{:?}", tests[2]);
}
