//! Dynamic tool registration: end-to-end coverage of the provider-registry
//! tool plane (JSON-RPC `tools/list` merging + `provider.tool` dispatch) and
//! the schema-first argument validation that guards every invocation.
//!
//! Covered cases (from the independent line-level review of PR #15):
//! - valid registration: a provider with a well-formed schema lists and
//!   invokes successfully
//! - duplicate registration: re-registering a provider id replaces it
//!   atomically; the same tool name on two providers stays distinct
//! - unknown tool / disabled provider: rejected with a JSON-RPC error,
//!   never a success envelope
//! - malformed schema: fails closed (never a silent pass-through)
//! - missing inputSchema: defaults to the permissive object schema
//! - schema mismatch / invalid arguments: rejected before the provider runs
//! - provider-side validation failure: surfaces as an error, not a success

use agent_workspace_hub::mcp::providers::McpClient;
use agent_workspace_hub::mcp::{
    validate_tool_arguments, CustomMcpProvider, GatewayProvider, McpDispatcher, SessionLifecycle,
    ToolCallResult, ToolContent, ToolDescriptor,
};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;

// ---------------------------------------------------------------- helpers

fn descriptor(name: &str, schema: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: String::new(),
        input_schema: schema,
    }
}

fn ok_result(text: &str) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolContent::Text {
            text: text.to_string(),
        }],
        is_error: false,
    }
}

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

async fn dispatch(dispatcher: &McpDispatcher, input: &str, lifecycle: &SessionLifecycle) -> Value {
    use agent_workspace_hub::mcp::DispatchResult;
    match dispatcher.dispatch_with_lifecycle(input, lifecycle).await {
        DispatchResult::Response(resp) => serde_json::to_value(&resp).expect("serialize"),
        DispatchResult::NoResponse => Value::Null,
    }
}

/// A dispatcher on a tempdir with one initialized session, ready for
/// `tools/list` / `tools/call`.
async fn ready_dispatcher() -> (McpDispatcher, SessionLifecycle, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let dispatcher = McpDispatcher::new_async(dir.path().to_path_buf())
        .await
        .expect("dispatcher");
    let lifecycle = SessionLifecycle::default();
    let input = request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "dynamic-tools-test", "version": "0.0.1"}
        }),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(
        response["result"].is_object(),
        "initialize failed: {response}"
    );
    (dispatcher, lifecycle, dir)
}

async fn list_tools(dispatcher: &McpDispatcher, lifecycle: &SessionLifecycle) -> Vec<Value> {
    let response = dispatch(
        dispatcher,
        &request(json!(90), "tools/list", json!({})),
        lifecycle,
    )
    .await;
    response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .clone()
}

async fn call_tool(
    dispatcher: &McpDispatcher,
    lifecycle: &SessionLifecycle,
    name: &str,
    arguments: Value,
) -> Value {
    dispatch(
        dispatcher,
        &request(
            json!(91),
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        ),
        lifecycle,
    )
    .await
}

fn find_tool<'a>(tools: &'a [Value], name: &str) -> Option<&'a Value> {
    tools.iter().find(|t| t["name"].as_str() == Some(name))
}

// ------------------------------------------------------- valid registration

/// A provider registered with a well-formed schema is listed with its
/// schema intact (prefix-qualified) and invoked with valid arguments.
#[tokio::test]
async fn valid_dynamic_tool_registers_lists_and_invokes() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "calc",
            || {
                Ok(vec![descriptor(
                    "add",
                    json!({
                        "type": "object",
                        "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}},
                        "required": ["x", "y"]
                    }),
                )])
            },
            |tool, args| {
                assert_eq!(tool, "add");
                let x = args["x"].as_i64().expect("x");
                let y = args["y"].as_i64().expect("y");
                Ok(ok_result(&format!("{}", x + y)))
            },
        )));

    let tools = list_tools(&dispatcher, &lifecycle).await;
    let add = find_tool(&tools, "calc.add").expect("calc.add must be listed");
    assert_eq!(add["inputSchema"]["required"][0], "x", "schema is listed");

    let response = call_tool(
        &dispatcher,
        &lifecycle,
        "calc.add",
        json!({"x": 2, "y": 40}),
    )
    .await;
    assert!(
        response["result"]["content"].is_array(),
        "valid args must succeed: {response}"
    );
    let text = serde_json::to_string(&response["result"]["content"]).expect("serialize");
    assert!(
        text.contains("42"),
        "result must carry the stub output: {text}"
    );
}

// ----------------------------------------------------- duplicate tool name

/// Re-registering the same provider id replaces the old provider entirely;
/// two providers exposing the same tool name remain distinct tools.
#[tokio::test]
async fn duplicate_provider_registration_replaces_by_id() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    let registry = dispatcher.provider_registry();

    registry
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "calc",
            || Ok(vec![descriptor("old", json!({"type": "object"}))]),
            |_t, _a| Ok(ok_result("old")),
        )));
    registry
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "calc",
            || Ok(vec![descriptor("new", json!({"type": "object"}))]),
            |t, _a| {
                if t == "new" {
                    Ok(ok_result("new tool"))
                } else {
                    anyhow::bail!("replaced provider no longer exposes '{t}'")
                }
            },
        )));
    // Same tool name on a different provider must coexist.
    registry
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "other",
            || Ok(vec![descriptor("new", json!({"type": "object"}))]),
            |_t, _a| Ok(ok_result("other-new")),
        )));

    let providers = registry.read().await.providers();
    assert_eq!(
        providers,
        vec!["calc".to_string(), "other".to_string()],
        "duplicate id must replace, not duplicate"
    );

    let tools = list_tools(&dispatcher, &lifecycle).await;
    assert!(find_tool(&tools, "calc.old").is_none(), "old tool is gone");
    let calc_new = find_tool(&tools, "calc.new").expect("calc.new listed");
    assert_eq!(
        calc_new["description"], "Tool provided by calc",
        "empty descriptions are filled with the provider id"
    );
    assert!(
        find_tool(&tools, "other.new").is_some(),
        "same tool name on another provider stays distinct"
    );

    let response = call_tool(&dispatcher, &lifecycle, "calc.new", json!({})).await;
    assert!(
        response["result"]["content"].is_array(),
        "replaced tool invokes"
    );
    // The stale registration no longer answers: only v2's tools exist.
    let response = call_tool(&dispatcher, &lifecycle, "calc.old", json!({})).await;
    assert!(
        response["error"].is_object(),
        "stale tool must error: {response}"
    );
}

// ------------------------------------------------------------ unknown tool

/// An unregistered provider (or an unlisted tool on a registered provider)
/// is rejected with a JSON-RPC error — never a success envelope.
#[tokio::test]
async fn unknown_dynamic_tool_is_rejected() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "calc",
            || Ok(vec![descriptor("add", json!({"type": "object"}))]),
            |t, _a| anyhow::bail!("provider does not expose '{t}'"),
        )));

    // Provider not registered at all: the lookup failure surfaces as a
    // JSON-RPC error envelope (never a fabricated success result).
    let response = call_tool(&dispatcher, &lifecycle, "ghost.tool", json!({})).await;
    let code = response["error"]["code"].as_i64().unwrap_or(0);
    assert_ne!(code, 0, "unknown provider must error, got: {response}");
    assert!(
        response["result"].is_null(),
        "unknown tool must not produce a result"
    );

    // Registered provider, unknown tool on it.
    let response = call_tool(&dispatcher, &lifecycle, "calc.nope", json!({})).await;
    assert!(response["error"].is_object(), "got: {response}");
    assert!(response["result"].is_null());
}

// --------------------------------------------------------- disabled provider

/// Unregistering (disabling) a provider removes its tools from the listing
/// and rejects further calls.
#[tokio::test]
async fn disabled_provider_is_removed_from_listing_and_calls() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    let registry = dispatcher.provider_registry();
    registry
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "calc",
            || Ok(vec![descriptor("add", json!({"type": "object"}))]),
            |_t, _a| Ok(ok_result("ok")),
        )));

    let tools = list_tools(&dispatcher, &lifecycle).await;
    assert!(
        find_tool(&tools, "calc.add").is_some(),
        "listed while enabled"
    );

    assert!(registry.write().await.unregister("calc"));
    let tools = list_tools(&dispatcher, &lifecycle).await;
    assert!(
        find_tool(&tools, "calc.add").is_none(),
        "unlisted after disable"
    );

    let response = call_tool(&dispatcher, &lifecycle, "calc.add", json!({})).await;
    assert!(
        response["error"].is_object(),
        "calls after disable must error"
    );
}

// ------------------------------------------------------ schema-first plane

/// Malformed advertised schemas fail closed at validation time — a string,
/// array, or null inputSchema is an error, not a permissive pass-through.
#[test]
fn malformed_advertised_schema_fails_closed() {
    let tools_response = json!({
        "tools": [
            {"name": "bad_string_schema", "inputSchema": "not an object"},
            {"name": "bad_null_schema", "inputSchema": null}
        ]
    });
    for tool in ["bad_string_schema", "bad_null_schema"] {
        let error = validate_tool_arguments(&tools_response, tool, &json!({}))
            .expect_err("malformed schema must fail closed");
        assert!(
            error.to_string().contains("schema"),
            "error must name the schema problem: {error:#}"
        );
    }
}

/// A tool with no inputSchema at all defaults to the permissive object
/// schema: any object arguments pass and reach the provider.
#[tokio::test]
async fn missing_input_schema_defaults_to_permissive_object() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    let invoked = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = Arc::clone(&invoked);
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "loose",
            || {
                Ok(vec![ToolDescriptor {
                    name: "freeform".into(),
                    description: String::new(),
                    input_schema: json!({"type": "object"}),
                }])
            },
            move |_t, a| {
                seen.lock().unwrap().push(a.clone());
                Ok(ok_result("ok"))
            },
        )));

    let response = call_tool(
        &dispatcher,
        &lifecycle,
        "loose.freeform",
        json!({"anything": ["goes", true]}),
    )
    .await;
    assert!(
        response["result"]["content"].is_array(),
        "missing-schema tool must accept object args: {response}"
    );
    assert_eq!(
        invoked.lock().unwrap()[0]["anything"][0],
        "goes",
        "provider received the arguments verbatim"
    );
}

/// Schema mismatch: arguments violating the advertised schema are rejected
/// before the provider is invoked; conforming arguments pass.
#[tokio::test]
async fn schema_mismatch_rejects_invalid_arguments_before_invoke() {
    let tools_response = json!({
        "tools": [{
            "name": "add",
            "inputSchema": {
                "type": "object",
                "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}},
                "required": ["x", "y"]
            }
        }]
    });

    // Missing required argument.
    let error = validate_tool_arguments(&tools_response, "add", &json!({"x": 1}))
        .expect_err("missing required must fail");
    assert!(
        error.to_string().contains("required"),
        "error must mention the missing field: {error:#}"
    );

    // Wrong type.
    let error = validate_tool_arguments(&tools_response, "add", &json!({"x": "1", "y": 2}))
        .expect_err("wrong type must fail");
    assert!(
        error.to_string().contains("x"),
        "error must point at the offending field: {error:#}"
    );

    // Conforming arguments pass.
    validate_tool_arguments(&tools_response, "add", &json!({"x": 1, "y": 2}))
        .expect("valid args must pass");

    // A tool that exists in the registry but not in the advertised list.
    let error = validate_tool_arguments(&tools_response, "absent", &json!({}))
        .expect_err("unadvertised tool must fail validation");
    assert!(
        error.to_string().contains("absent"),
        "error must name the tool: {error:#}"
    );
}

// ------------------------------------------------ provider-side validation

/// An McpClient stub mirroring `StdioMcpClient`: it validates arguments
/// against the live schema BEFORE forwarding, so a provider-side
/// validation failure must surface as a JSON-RPC error, not a success.
struct ValidatingStubClient {
    tools: Value,
}

#[async_trait]
impl McpClient for ValidatingStubClient {
    async fn tools_list(&self) -> Result<Value> {
        Ok(self.tools.clone())
    }
    async fn tools_call(&self, tool: &str, args: Value) -> Result<Value> {
        validate_tool_arguments(&self.tools, tool, &args)?;
        Ok(json!({
            "content": [{"type": "text", "text": format!("{tool} called")}],
            "isError": false
        }))
    }
}

/// Provider-side validation failures (the path every custom MCP server
/// takes) must propagate as errors, never silently succeed.
#[tokio::test]
async fn provider_side_validation_failure_surfaces_as_error() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    let stub = ValidatingStubClient {
        tools: json!({
            "tools": [{
                "name": "guarded",
                "inputSchema": {
                    "type": "object",
                    "properties": {"n": {"type": "integer"}},
                    "required": ["n"]
                }
            }]
        }),
    };
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(CustomMcpProvider::new("guard", Arc::new(stub))));

    let tools = list_tools(&dispatcher, &lifecycle).await;
    assert!(find_tool(&tools, "guard.guarded").is_some());

    // Invalid arguments: the provider-side check rejects before invoking.
    let response = call_tool(
        &dispatcher,
        &lifecycle,
        "guard.guarded",
        json!({"n": "one"}),
    )
    .await;
    assert!(
        response["error"].is_object(),
        "provider-side validation failure must be an error: {response}"
    );

    // Valid arguments reach the (stubbed) provider.
    let response = call_tool(&dispatcher, &lifecycle, "guard.guarded", json!({"n": 1})).await;
    assert!(
        response["result"]["content"].is_array(),
        "valid args must succeed: {response}"
    );
}

/// A dynamic tool whose advertised schema is malformed is NOT exposed in
/// `tools/list` (fail closed per tool), while its healthy siblings stay
/// listed, and direct calls to the dropped tool are rejected.
#[tokio::test]
async fn malformed_dynamic_tool_schema_is_not_exposed() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "mix",
            || {
                Ok(vec![
                    descriptor("good", json!({"type": "object"})),
                    // Schema is a string, not an object.
                    ToolDescriptor {
                        name: "bad_string".into(),
                        description: String::new(),
                        input_schema: json!("not a schema"),
                    },
                    // `required` is not an array of strings.
                    ToolDescriptor {
                        name: "bad_required".into(),
                        description: String::new(),
                        input_schema: json!({"type": "object", "required": "oops"}),
                    },
                    // Unsupported keyword: cannot be validated by AWH.
                    ToolDescriptor {
                        name: "bad_ref".into(),
                        description: String::new(),
                        input_schema: json!({"$ref": "#/definitions/x"}),
                    },
                ])
            },
            |t, _a| Ok(ok_result(t)),
        )));

    let tools = list_tools(&dispatcher, &lifecycle).await;
    assert!(
        find_tool(&tools, "mix.good").is_some(),
        "healthy tool listed"
    );
    assert!(
        find_tool(&tools, "mix.bad_string").is_none(),
        "malformed schema must not be exposed"
    );
    assert!(
        find_tool(&tools, "mix.bad_required").is_none(),
        "non-array required dropped"
    );
    assert!(
        find_tool(&tools, "mix.bad_ref").is_none(),
        "unsupported keyword dropped"
    );

    // Direct calls to a dropped tool also fail closed (they must not be
    // invocable just because the provider would answer).
    let response = call_tool(&dispatcher, &lifecycle, "mix.bad_string", json!({})).await;
    assert!(
        response["error"].is_object(),
        "dropped tool must not be invocable: {response}"
    );
}

/// AWH-side validation is independent of the provider: a provider that
/// would accept ANY arguments (no validation of its own) still cannot be
/// invoked with arguments that violate its advertised schema. The
/// provider's seen-list proves it was never reached.
#[tokio::test]
async fn awh_side_validation_blocks_bad_args_even_when_provider_would_allow() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    let invoked = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = Arc::clone(&invoked);
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "permissive",
            || {
                Ok(vec![descriptor(
                    "guarded",
                    json!({
                        "type": "object",
                        "properties": {"n": {"type": "integer"}},
                        "required": ["n"]
                    }),
                )])
            },
            move |_t, a| {
                seen.lock().unwrap().push(a.clone());
                Ok(ok_result("provider executed"))
            },
        )));

    // Violating arguments: rejected by AWH before the provider runs.
    let response = call_tool(
        &dispatcher,
        &lifecycle,
        "permissive.guarded",
        json!({"n": "one"}),
    )
    .await;
    assert!(
        response["error"].is_object(),
        "AWH-side validation must reject: {response}"
    );
    assert!(
        response["result"].is_null(),
        "no success envelope for rejected arguments"
    );
    assert!(
        invoked.lock().unwrap().is_empty(),
        "the provider must never have been invoked"
    );

    // The same tool via the generic connector.invoke path is gated too —
    // it cannot be used to bypass the direct-call validation.
    let response = dispatch(
        &dispatcher,
        &request(
            json!(92),
            "tools/call",
            json!({
                "name": "connector.invoke",
                "arguments": {"provider": "permissive", "tool": "guarded", "arguments": {"n": "one"}}
            }),
        ),
        &lifecycle,
    )
    .await;
    assert!(
        response["error"].is_object(),
        "connector.invoke must not bypass schema validation: {response}"
    );
    assert!(
        invoked.lock().unwrap().is_empty(),
        "connector.invoke path must also block before the provider"
    );

    // Conforming arguments reach the provider.
    let response = call_tool(
        &dispatcher,
        &lifecycle,
        "permissive.guarded",
        json!({"n": 3}),
    )
    .await;
    assert!(
        response["result"]["content"].is_array(),
        "valid args pass: {response}"
    );
    assert_eq!(
        invoked.lock().unwrap().len(),
        1,
        "provider invoked exactly once"
    );
}

/// A provider whose invocation itself errors must surface the error as a
/// JSON-RPC error envelope — a broken connector is never reported as a
/// successful tool call.
#[tokio::test]
async fn provider_invoke_error_maps_to_jsonrpc_error() {
    let (dispatcher, lifecycle, _dir) = ready_dispatcher().await;
    dispatcher
        .provider_registry()
        .write()
        .await
        .register(Box::new(GatewayProvider::new(
            "broken",
            || Ok(vec![descriptor("boom", json!({"type": "object"}))]),
            |_t, _a| anyhow::bail!("provider exploded"),
        )));

    let response = call_tool(&dispatcher, &lifecycle, "broken.boom", json!({})).await;
    assert!(response["error"].is_object(), "got: {response}");
    assert!(response["result"].is_null());
}
