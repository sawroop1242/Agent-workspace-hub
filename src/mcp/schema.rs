use anyhow::{bail, Context, Result};
use serde_json::Value;

const MAX_SCHEMA_DEPTH: usize = 32;

/// Validates tool-call arguments against the tool's advertised `inputSchema`,
/// rejecting invalid arguments before the MCP tool is invoked.
pub fn validate_tool_arguments(
    tools_response: &Value,
    tool_name: &str,
    arguments: &Value,
) -> Result<()> {
    let tools = tools_response
        .get("tools")
        .and_then(Value::as_array)
        .context("MCP tools/list response has no tools array")?;
    let tool = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
        .with_context(|| format!("MCP tool not found: {tool_name}"))?;
    let schema = tool
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object"}));
    validate_json_schema(&schema, arguments, 0, "arguments")
}

fn validate_json_schema(schema: &Value, value: &Value, depth: usize, path: &str) -> Result<()> {
    if depth > MAX_SCHEMA_DEPTH {
        bail!("MCP argument validation exceeded maximum nesting depth");
    }

    if let Some(schema_type) = schema.get("type") {
        let matches = match schema_type {
            Value::String(kind) => type_matches(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| type_matches(kind, value)),
            _ => false,
        };
        if !matches {
            bail!("MCP argument validation failed at {path}: expected schema type");
        }
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.iter().any(|candidate| candidate == value) {
            bail!("MCP argument validation failed at {path}: value is not allowed");
        }
    }

    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    bail!(
                        "MCP argument validation failed at {path}: missing required field '{field}'"
                    );
                }
            }
        }

        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (field, child_schema) in properties {
                if let Some(child) = object.get(field) {
                    validate_json_schema(
                        child_schema,
                        child,
                        depth + 1,
                        &format!("{path}.{field}"),
                    )?;
                }
            }

            if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
                for field in object.keys() {
                    if !properties.contains_key(field) {
                        bail!("MCP argument validation failed at {path}: unknown field '{field}'");
                    }
                }
            }
        }
    }

    if let Some(item_schema) = schema.get("items") {
        if let Some(array) = value.as_array() {
            for (index, item) in array.iter().enumerate() {
                validate_json_schema(item_schema, item, depth + 1, &format!("{path}[{index}]"))?;
            }
        }
    }

    Ok(())
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "tools": [{
                "name": "sum",
                "inputSchema": {
                    "type": "object",
                    "required": ["a"],
                    "properties": {
                        "a": {"type": "integer"},
                        "b": {"type": "string"}
                    },
                    "additionalProperties": false
                }
            }]
        })
    }

    #[test]
    fn accepts_valid_arguments() {
        assert!(validate_tool_arguments(&schema(), "sum", &json!({"a": 4, "b": "x"})).is_ok());
    }

    #[test]
    fn rejects_missing_required_argument() {
        assert!(validate_tool_arguments(&schema(), "sum", &json!({})).is_err());
    }

    #[test]
    fn rejects_wrong_type_and_unknown_field() {
        assert!(validate_tool_arguments(&schema(), "sum", &json!({"a": "4"})).is_err());
        assert!(validate_tool_arguments(&schema(), "sum", &json!({"a": 4, "x": true})).is_err());
    }

    #[test]
    fn missing_tools_array_is_rejected() {
        assert!(validate_tool_arguments(&json!({}), "sum", &json!({"a": 1})).is_err());
    }

    #[test]
    fn unknown_tool_is_rejected() {
        assert!(validate_tool_arguments(&schema(), "nope", &json!({"a": 1})).is_err());
    }

    #[test]
    fn enum_values_are_enforced() {
        let response = json!({
            "tools": [{
                "name": "level",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "level": {"type": "string", "enum": ["low", "high"]}
                    }
                }
            }]
        });
        assert!(validate_tool_arguments(&response, "level", &json!({"level": "low"})).is_ok());
        assert!(validate_tool_arguments(&response, "level", &json!({"level": "medium"})).is_err());
    }

    #[test]
    fn nested_objects_and_arrays_are_validated() {
        let response = json!({
            "tools": [{
                "name": "grid",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "cell": {
                            "type": "object",
                            "properties": {
                                "coords": {"type": "array", "items": {"type": "integer"}}
                            }
                        }
                    }
                }
            }]
        });
        assert!(validate_tool_arguments(
            &response,
            "grid",
            &json!({"cell": {"coords": [1, 2, 3]}})
        )
        .is_ok());
        // An item of the wrong type must fail.
        assert!(
            validate_tool_arguments(&response, "grid", &json!({"cell": {"coords": [1, "2"]}}))
                .is_err()
        );
    }

    #[test]
    fn multi_type_union_accepts_any_listed_type() {
        let response = json!({
            "tools": [{
                "name": "flex",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "value": {"type": ["string", "integer"]}
                    }
                }
            }]
        });
        assert!(validate_tool_arguments(&response, "flex", &json!({"value": "x"})).is_ok());
        assert!(validate_tool_arguments(&response, "flex", &json!({"value": 42})).is_ok());
        assert!(validate_tool_arguments(&response, "flex", &json!({"value": true})).is_err());
    }

    #[test]
    fn excessive_nesting_depth_is_rejected() {
        // Build a schema nested deeper than MAX_SCHEMA_DEPTH; resolution follows
        // the schema tree, so the deeply nested value must also match it.
        let mut schema = serde_json::json!({"type": "object", "properties": {}});
        for _ in 0..=MAX_SCHEMA_DEPTH {
            schema = json!({"type": "object", "properties": {"next": schema}});
        }
        let mut value = json!(0);
        for _ in 0..=MAX_SCHEMA_DEPTH {
            value = json!({"next": value});
        }
        let response = json!({ "tools": [{ "name": "deep", "inputSchema": schema }] });
        assert!(validate_tool_arguments(&response, "deep", &value).is_err());
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        // The validator must never panic on arbitrary JSON values.
        proptest! {
            #[test]
            fn tool_validation_never_panics_on_arbitrary_input(value in any_json()) {
                let response = json!({
                    "tools": [{
                        "name": "sum",
                        "inputSchema": {
                            "type": "object",
                            "required": ["a"],
                            "properties": {
                                "a": {"type": "integer"},
                                "b": {"type": "string"}
                            },
                            "additionalProperties": false
                        }
                    }]
                });
                // Must not panic; result is irrelevant.
                let _ = validate_tool_arguments(&response, "sum", &value);
            }
        }

        /// Generates a bounded, recursive, arbitrary JSON value.
        fn any_json() -> impl Strategy<Value = serde_json::Value> {
            let leaf = prop_oneof![
                Just(serde_json::Value::Null),
                any::<bool>().prop_map(serde_json::Value::Bool),
                any::<i64>().prop_map(serde_json::Value::from),
                any::<f64>().prop_map(serde_json::Value::from),
                "[a-zA-Z0-9]{0,16}".prop_map(serde_json::Value::String),
            ];
            leaf.prop_recursive(4, 16, 4, |inner| {
                prop_oneof![
                    proptest::collection::vec(inner.clone(), 0..4)
                        .prop_map(serde_json::Value::Array),
                    proptest::collection::vec(
                        ("[a-z]{1,8}".prop_map(String::from), inner.clone()),
                        0..4,
                    )
                    .prop_map(|pairs| {
                        serde_json::Value::Object(
                            pairs.into_iter().collect::<serde_json::Map<_, _>>(),
                        )
                    }),
                ]
            })
        }
    }
}
