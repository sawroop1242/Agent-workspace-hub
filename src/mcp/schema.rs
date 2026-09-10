//! Tool argument validation against JSON-Schema-style `inputSchema` documents.
//!
//! This is a **safe subset** of JSON Schema (see docs/mcp.md for the exact
//! keyword matrix). Design constraints, all load-bearing for security:
//!
//! * **Bounded recursion** — schema/instance traversal is capped at
//!   [`MAX_SCHEMA_DEPTH`]; deeper documents fail closed instead of
//!   exhausting the stack.
//! * **No regex engine dependency** — `pattern` is matched by a purpose-built
//!   linear-time NFA simulation (the [`regex`] submodule). By construction
//!   it cannot backtrack, so catastrophic backtracking is impossible.
//!     Constructs the subset does not model (backreferences, lookaround,
//!     word boundaries, Unicode property classes) fail closed with an
//!     explicit error rather than being silently skipped.
//! * **Structured errors** — every failure carries the failing
//!   [`SchemaKeyword`], the instance path, and the schema path, so callers
//!   can surface precise diagnostics without echoing argument values.
//!
//! Unsupported keywords are listed in [`UNSUPPORTED_SCHEMA_KEYWORDS`]; when
//! one appears in a schema, validation fails closed with a clear error
//! instead of ignoring the keyword.

use anyhow::{Context, Result};
use serde_json::Value;

/// Maximum schema/instance nesting depth the validator will traverse.
pub const MAX_SCHEMA_DEPTH: usize = 32;
/// Maximum string length (bytes) `pattern` will match against. Longer
/// strings fail closed; this is a documented limitation of `pattern`
/// support in this milestone.
pub const MAX_PATTERN_INPUT_BYTES: usize = 64 * 1024;
/// Maximum characters accepted in a `pattern` expression.
pub const MAX_PATTERN_LENGTH: usize = 512;

/// JSON Schema keywords this validator implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaKeyword {
    /// `type`
    Type,
    /// `enum`
    Enum,
    /// `const`
    Const,
    /// `required`
    Required,
    /// `properties`
    Properties,
    /// `additionalProperties`
    AdditionalProperties,
    /// `patternProperties`
    PatternProperties,
    /// `propertyNames`
    PropertyNames,
    /// `minProperties`
    MinProperties,
    /// `maxProperties`
    MaxProperties,
    /// `dependentRequired`
    DependentRequired,
    /// `items`
    Items,
    /// `prefixItems`
    PrefixItems,
    /// `minItems`
    MinItems,
    /// `maxItems`
    MaxItems,
    /// `uniqueItems`
    UniqueItems,
    /// `contains`
    Contains,
    /// `minContains`
    MinContains,
    /// `maxContains`
    MaxContains,
    /// `minLength`
    MinLength,
    /// `maxLength`
    MaxLength,
    /// `pattern`
    Pattern,
    /// `minimum`
    Minimum,
    /// `maximum`
    Maximum,
    /// `exclusiveMinimum`
    ExclusiveMinimum,
    /// `exclusiveMaximum`
    ExclusiveMaximum,
    /// `multipleOf`
    MultipleOf,
    /// `allOf`
    AllOf,
    /// `anyOf`
    AnyOf,
    /// `oneOf`
    OneOf,
    /// `not`
    Not,
    /// `if`/`then`/`else`
    If,
    /// A schema-structure fault (e.g. a keyword with the wrong JSON type),
    /// not a constraint violation in the instance.
    MalformedSchema,
    /// The schema uses a keyword this validator deliberately does not
    /// support; validation fails closed.
    UnsupportedKeyword,
    /// A guardrail tripped (pattern budget, malformed regex, depth cap).
    ResourceLimit,
}

impl SchemaKeyword {
    /// The JSON Schema keyword name this enum member represents.
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaKeyword::Type => "type",
            SchemaKeyword::Enum => "enum",
            SchemaKeyword::Const => "const",
            SchemaKeyword::Required => "required",
            SchemaKeyword::Properties => "properties",
            SchemaKeyword::AdditionalProperties => "additionalProperties",
            SchemaKeyword::PatternProperties => "patternProperties",
            SchemaKeyword::PropertyNames => "propertyNames",
            SchemaKeyword::MinProperties => "minProperties",
            SchemaKeyword::MaxProperties => "maxProperties",
            SchemaKeyword::DependentRequired => "dependentRequired",
            SchemaKeyword::Items => "items",
            SchemaKeyword::PrefixItems => "prefixItems",
            SchemaKeyword::MinItems => "minItems",
            SchemaKeyword::MaxItems => "maxItems",
            SchemaKeyword::UniqueItems => "uniqueItems",
            SchemaKeyword::Contains => "contains",
            SchemaKeyword::MinContains => "minContains",
            SchemaKeyword::MaxContains => "maxContains",
            SchemaKeyword::MinLength => "minLength",
            SchemaKeyword::MaxLength => "maxLength",
            SchemaKeyword::Pattern => "pattern",
            SchemaKeyword::Minimum => "minimum",
            SchemaKeyword::Maximum => "maximum",
            SchemaKeyword::ExclusiveMinimum => "exclusiveMinimum",
            SchemaKeyword::ExclusiveMaximum => "exclusiveMaximum",
            SchemaKeyword::MultipleOf => "multipleOf",
            SchemaKeyword::AllOf => "allOf",
            SchemaKeyword::AnyOf => "anyOf",
            SchemaKeyword::OneOf => "oneOf",
            SchemaKeyword::Not => "not",
            SchemaKeyword::If => "if",
            SchemaKeyword::MalformedSchema => "malformedSchema",
            SchemaKeyword::UnsupportedKeyword => "unsupportedKeyword",
            SchemaKeyword::ResourceLimit => "resourceLimit",
        }
    }
}

/// Keywords deliberately not supported. Any schema containing one of these
/// fails closed — the keyword is never silently ignored.
///
/// * `$ref`/`$defs`/`$dynamicRef`/`$dynamicAnchor`: reference resolution
///   would allow cyclic schemas; unsupported in this milestone.
/// * `format`: annotation-only in the drafts this validator targets, so
///   it is rejected loudly so schema authors get a signal (implementing
///   specific formats is future work).
/// * `unevaluatedProperties`/`unevaluatedItems`: require full annotation
///   tracking through combinators.
/// * `contentEncoding`/`contentMediaType`: string-content assertions
///   beyond `pattern`.
/// * `dependentSchemas`: conditional subschemas on object shape; only the
///   simpler `dependentRequired` is supported.
pub const UNSUPPORTED_SCHEMA_KEYWORDS: [&str; 10] = [
    "$ref",
    "$defs",
    "$dynamicRef",
    "$dynamicAnchor",
    "format",
    "unevaluatedProperties",
    "unevaluatedItems",
    "contentEncoding",
    "contentMediaType",
    "dependentSchemas",
];

/// A structured validation failure: which keyword, where in the instance,
/// where in the schema, and a human-readable reason.
#[derive(Debug, Clone)]
pub struct SchemaError {
    /// The keyword that failed.
    pub keyword: SchemaKeyword,
    /// Path into the validated instance (JSONPath-ish: `$.files[2].name`,
    /// or `arguments` at the root).
    pub instance_path: String,
    /// Path into the schema document (`#/properties/files/items`).
    pub schema_path: String,
    /// Human-readable description of the failure. Never echoes instance
    /// values, so it cannot leak argument content.
    pub message: String,
}

impl SchemaError {
    fn new(
        keyword: SchemaKeyword,
        instance_path: impl Into<String>,
        schema_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            keyword,
            instance_path: instance_path.into(),
            schema_path: schema_path.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MCP argument validation failed at {}: {} (keyword: {}, schema: {})",
            self.instance_path,
            self.message,
            self.keyword.as_str(),
            self.schema_path
        )
    }
}

impl std::error::Error for SchemaError {}

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
    validate_schema(&schema, arguments, "arguments", "#").map_err(anyhow::Error::new)
}

/// Syntax-only schema check: walks the SCHEMA DOCUMENT itself (no instance)
/// and fails closed on structurally malformed metadata — wrong JSON types
/// for keywords, bad `required` entries, uncompilable `pattern`s, nested
/// subschemas that are not objects — as well as keywords this validator
/// does not support (so such a tool is never advertised as valid).
///
/// Unknown keywords are ignored, matching JSON Schema's annotation
/// semantics. Used as the dynamic-tool exposure gate: a tool whose
/// advertised schema cannot be validated must not be exposed.
pub fn validate_schema_syntax(schema: &Value, schema_path: &str) -> Result<(), SchemaError> {
    validate_schema_syntax_inner(schema, schema_path, 0)
}

const SCHEMA_SYNTAX_DEPTH_LIMIT: usize = MAX_SCHEMA_DEPTH;

fn validate_schema_syntax_inner(
    schema: &Value,
    schema_path: &str,
    depth: usize,
) -> Result<(), SchemaError> {
    if depth >= SCHEMA_SYNTAX_DEPTH_LIMIT {
        return Err(SchemaError::new(
            SchemaKeyword::ResourceLimit,
            "schema",
            schema_path,
            format!("schema exceeded the {SCHEMA_SYNTAX_DEPTH_LIMIT}-level nesting limit"),
        ));
    }
    let object = schema.as_object().ok_or_else(|| {
        SchemaError::new(
            SchemaKeyword::MalformedSchema,
            "schema",
            schema_path,
            "schema must be a JSON object",
        )
    })?;
    for keyword in UNSUPPORTED_SCHEMA_KEYWORDS {
        if object.contains_key(keyword) {
            return Err(SchemaError::new(
                SchemaKeyword::UnsupportedKeyword,
                "schema",
                format!("{schema_path}/{keyword}"),
                format!("schema keyword '{keyword}' is not supported by this validator"),
            ));
        }
    }
    for (keyword, value) in object {
        let child_path = format!("{schema_path}/{keyword}");
        let malformed = |message: &str| {
            Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                "schema",
                child_path.clone(),
                message.to_string(),
            ))
        };
        match keyword.as_str() {
            // Subschemas: recurse (a schema must itself be an object).
            "properties" | "patternProperties" => {
                if !value.is_object() {
                    return malformed("must be an object mapping keys to schemas");
                }
                for (key, sub) in value.as_object().expect("object") {
                    validate_schema_syntax_inner(sub, &format!("{child_path}/{key}"), depth + 1)?;
                }
            }
            "additionalProperties" | "propertyNames" | "items" | "not" | "if" | "then" | "else" => {
                if value.is_boolean() && keyword.as_str() == "additionalProperties" {
                    continue;
                }
                validate_schema_syntax_inner(value, &child_path, depth + 1)?;
            }
            "prefixItems" | "allOf" | "anyOf" | "oneOf" => {
                let Some(items) = value.as_array() else {
                    return malformed("must be an array of schemas");
                };
                for (index, sub) in items.iter().enumerate() {
                    validate_schema_syntax_inner(sub, &format!("{child_path}/{index}"), depth + 1)?;
                }
            }
            // Typed keyword values.
            "type" => {
                let ok = value.as_str().is_some()
                    || value
                        .as_array()
                        .is_some_and(|arr| !arr.is_empty() && arr.iter().all(Value::is_string));
                if !ok {
                    return malformed("must be a string or a non-empty array of strings");
                }
            }
            "enum" => {
                if !value.is_array() {
                    return malformed("must be an array");
                }
            }
            "required" => {
                let ok = value
                    .as_array()
                    .is_some_and(|arr| arr.iter().all(Value::is_string));
                if !ok {
                    return malformed("must be an array of property names");
                }
            }
            "dependentRequired" => {
                let ok = value.as_object().is_some_and(|map| {
                    map.values().all(|v| {
                        v.as_array()
                            .is_some_and(|arr| arr.iter().all(Value::is_string))
                    })
                });
                if !ok {
                    return malformed("must map property names to arrays of names");
                }
            }
            "pattern" => {
                let Some(pattern) = value.as_str() else {
                    return malformed("must be a string");
                };
                if let Err(message) = regex::matches(pattern, "") {
                    return Err(SchemaError::new(
                        SchemaKeyword::MalformedSchema,
                        "schema",
                        child_path,
                        format!("pattern does not compile: {message}"),
                    ));
                }
            }
            "minItems" | "maxItems" | "minContains" | "maxContains" | "minLength" | "maxLength"
            | "minProperties" | "maxProperties" => {
                if !value.is_u64() {
                    return malformed("must be a non-negative integer");
                }
            }
            "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf" => {
                if !value.is_number()
                    || (keyword.as_str() == "multipleOf" && value.as_f64() == Some(0.0))
                {
                    return malformed("must be a number (multipleOf > 0)");
                }
            }
            "uniqueItems" | "deprecated" => {
                if !value.is_boolean() {
                    return malformed("must be a boolean");
                }
            }
            // Annotations and identifiers: any value is acceptable.
            "const" | "title" | "description" | "default" | "examples" | "$schema" | "$id"
            | "$comment" | "contains" => {}
            // Unknown keywords are annotations per JSON Schema: ignored.
            _ => {}
        }
    }
    Ok(())
}

/// Validates `value` against `schema`, producing a structured
/// [`SchemaError`] on the first violation.
pub fn validate_schema(
    schema: &Value,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    let _depth_guard = DepthGuard::acquire()?;

    // A schema must itself be a JSON object. (A `true`/`false` boolean
    // schema is legal in full JSON Schema; this validator does not
    // support it, so it fails closed — an explicit, documented
    // limitation.)
    let object = schema.as_object().ok_or_else(|| {
        SchemaError::new(
            SchemaKeyword::MalformedSchema,
            instance_path,
            schema_path,
            "schema must be a JSON object",
        )
    })?;

    // Unsupported keywords fail closed, loudly.
    for keyword in UNSUPPORTED_SCHEMA_KEYWORDS {
        if object.contains_key(keyword) {
            return Err(SchemaError::new(
                SchemaKeyword::UnsupportedKeyword,
                instance_path,
                format!("{schema_path}/{keyword}"),
                format!("schema keyword '{keyword}' is not supported by this validator"),
            ));
        }
    }

    validate_type(object, value, instance_path, schema_path)?;
    validate_const_and_enum(object, value, instance_path, schema_path)?;
    validate_numbers(object, value, instance_path, schema_path)?;
    validate_strings(object, value, instance_path, schema_path)?;

    if let Some(map) = value.as_object() {
        validate_object_keywords(object, map, instance_path, schema_path)?;
    }
    if let Some(array) = value.as_array() {
        validate_array_keywords(object, array, instance_path, schema_path)?;
    }

    validate_combinators(object, value, instance_path, schema_path)?;
    Ok(())
}

/// Shared depth counter for [`DepthGuard`]. Module-scoped so `acquire` and
/// `Drop` reference the SAME static (function-local statics would be
/// distinct items and the counter would never balance).
static VALIDATION_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Bounded-recursion guard: each nested schema application consumes one
/// level; beyond [`MAX_SCHEMA_DEPTH`] validation fails closed instead of
/// exhausting the stack.
struct DepthGuard {
    _private: (),
}

impl DepthGuard {
    fn acquire() -> Result<Self, SchemaError> {
        let current = VALIDATION_DEPTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if current >= MAX_SCHEMA_DEPTH {
            VALIDATION_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            return Err(SchemaError::new(
                SchemaKeyword::ResourceLimit,
                "arguments",
                "#",
                format!("schema validation exceeded maximum nesting depth ({MAX_SCHEMA_DEPTH})"),
            ));
        }
        Ok(Self { _private: () })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        VALIDATION_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn validate_type(
    object: &serde_json::Map<String, Value>,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    let Some(schema_type) = object.get("type") else {
        return Ok(());
    };
    let matches = match schema_type {
        Value::String(kind) => type_matches(kind, value),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| type_matches(kind, value)),
        _ => {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/type"),
                "schema 'type' must be a string or array of strings",
            ))
        }
    };
    if !matches {
        return Err(SchemaError::new(
            SchemaKeyword::Type,
            instance_path,
            format!("{schema_path}/type"),
            "expected schema type",
        ));
    }
    Ok(())
}

fn validate_const_and_enum(
    object: &serde_json::Map<String, Value>,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    if let Some(expected) = object.get("const") {
        if expected != value {
            return Err(SchemaError::new(
                SchemaKeyword::Const,
                instance_path,
                format!("{schema_path}/const"),
                "value does not match the required constant",
            ));
        }
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        if !values.iter().any(|candidate| candidate == value) {
            return Err(SchemaError::new(
                SchemaKeyword::Enum,
                instance_path,
                format!("{schema_path}/enum"),
                "value is not allowed",
            ));
        }
    }
    Ok(())
}

fn validate_numbers(
    object: &serde_json::Map<String, Value>,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    let Some(number) = value.as_f64() else {
        return Ok(());
    };
    if let Some(factor) = number_keyword(object, "multipleOf", instance_path, schema_path)? {
        if !is_multiple_of(number, factor) {
            return Err(SchemaError::new(
                SchemaKeyword::MultipleOf,
                instance_path,
                format!("{schema_path}/multipleOf"),
                "value is not a multiple of the required factor",
            ));
        }
    }
    if let Some(bound) = number_keyword(object, "minimum", instance_path, schema_path)? {
        if number < bound {
            return Err(SchemaError::new(
                SchemaKeyword::Minimum,
                instance_path,
                format!("{schema_path}/minimum"),
                "value is below the required minimum",
            ));
        }
    }
    if let Some(bound) = number_keyword(object, "maximum", instance_path, schema_path)? {
        if number > bound {
            return Err(SchemaError::new(
                SchemaKeyword::Maximum,
                instance_path,
                format!("{schema_path}/maximum"),
                "value is above the required maximum",
            ));
        }
    }
    // Number form (draft 2020-12). The legacy draft-04 boolean form is
    // rejected as malformed below via `number_keyword`.
    if let Some(bound) = number_keyword(object, "exclusiveMinimum", instance_path, schema_path)? {
        if number <= bound {
            return Err(SchemaError::new(
                SchemaKeyword::ExclusiveMinimum,
                instance_path,
                format!("{schema_path}/exclusiveMinimum"),
                "value is not above the exclusive minimum",
            ));
        }
    }
    if let Some(bound) = number_keyword(object, "exclusiveMaximum", instance_path, schema_path)? {
        if number >= bound {
            return Err(SchemaError::new(
                SchemaKeyword::ExclusiveMaximum,
                instance_path,
                format!("{schema_path}/exclusiveMaximum"),
                "value is not below the exclusive maximum",
            ));
        }
    }
    Ok(())
}

fn validate_strings(
    object: &serde_json::Map<String, Value>,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    if let Some(min) = usize_keyword(object, "minLength", instance_path, schema_path)? {
        if text.chars().count() < min {
            return Err(SchemaError::new(
                SchemaKeyword::MinLength,
                instance_path,
                format!("{schema_path}/minLength"),
                "string is shorter than required",
            ));
        }
    }
    if let Some(max) = usize_keyword(object, "maxLength", instance_path, schema_path)? {
        if text.chars().count() > max {
            return Err(SchemaError::new(
                SchemaKeyword::MaxLength,
                instance_path,
                format!("{schema_path}/maxLength"),
                "string is longer than allowed",
            ));
        }
    }
    if let Some(pattern_value) = object.get("pattern") {
        let pattern = pattern_value.as_str().ok_or_else(|| {
            SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/pattern"),
                "schema 'pattern' must be a string",
            )
        })?;
        match regex::matches(pattern, text) {
            Ok(true) => {}
            Ok(false) => {
                return Err(SchemaError::new(
                    SchemaKeyword::Pattern,
                    instance_path,
                    format!("{schema_path}/pattern"),
                    "string does not match the required pattern",
                ))
            }
            Err(message) => {
                return Err(SchemaError::new(
                    SchemaKeyword::ResourceLimit,
                    instance_path,
                    format!("{schema_path}/pattern"),
                    message,
                ))
            }
        }
    }
    Ok(())
}

/// Validates the object keywords (`properties`, `required`,
/// `additionalProperties`, `patternProperties`, `propertyNames`,
/// `min/maxProperties`, `dependentRequired`).
fn validate_object_keywords(
    schema: &serde_json::Map<String, Value>,
    map: &serde_json::Map<String, Value>,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    if let Some(required) = schema.get("required") {
        let required = required.as_array().ok_or_else(|| {
            SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/required"),
                "schema 'required' must be an array of strings",
            )
        })?;
        if required.iter().any(|entry| !entry.is_string()) {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/required"),
                "schema 'required' must be an array of strings",
            ));
        }
        for field in required.iter().filter_map(Value::as_str) {
            if !map.contains_key(field) {
                return Err(SchemaError::new(
                    SchemaKeyword::Required,
                    instance_path,
                    format!("{schema_path}/required"),
                    format!("missing required field '{field}'"),
                ));
            }
        }
    }
    if let Some(properties) = schema.get("properties") {
        let properties = properties.as_object().ok_or_else(|| {
            SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/properties"),
                "schema 'properties' must be an object",
            )
        })?;
        for (field, child_schema) in properties {
            if let Some(child) = map.get(field) {
                validate_schema(
                    child_schema,
                    child,
                    &format!("{instance_path}.{field}"),
                    &format!("{schema_path}/properties/{field}"),
                )?;
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    let patterns = schema.get("patternProperties").and_then(Value::as_object);
    let key_matches_declaration = |field: &str| -> bool {
        let declared = properties
            .map(|properties| properties.contains_key(field))
            .unwrap_or(false);
        let patterned = patterns
            .map(|patterns| {
                patterns
                    .keys()
                    .any(|pattern| regex::matches(pattern, field).unwrap_or(false))
            })
            .unwrap_or(false);
        declared || patterned
    };
    if let Some(additional) = schema.get("additionalProperties") {
        match additional {
            Value::Bool(false) => {
                for field in map.keys() {
                    if !key_matches_declaration(field) {
                        return Err(SchemaError::new(
                            SchemaKeyword::AdditionalProperties,
                            instance_path,
                            format!("{schema_path}/additionalProperties"),
                            format!("unknown field '{field}'"),
                        ));
                    }
                }
            }
            Value::Bool(true) => {}
            Value::Object(_) => {
                // Schema form: undeclared keys must satisfy the subschema.
                for (field, child) in map {
                    if !key_matches_declaration(field) {
                        validate_schema(
                            additional,
                            child,
                            &format!("{instance_path}.{field}"),
                            &format!("{schema_path}/additionalProperties"),
                        )?;
                    }
                }
            }
            _ => {
                return Err(SchemaError::new(
                    SchemaKeyword::MalformedSchema,
                    instance_path,
                    format!("{schema_path}/additionalProperties"),
                    "schema 'additionalProperties' must be a boolean or object",
                ))
            }
        }
    }
    if let Some(patterns) = patterns {
        for (pattern, child_schema) in patterns {
            for (field, child) in map {
                match regex::matches(pattern, field) {
                    Ok(true) => validate_schema(
                        child_schema,
                        child,
                        &format!("{instance_path}.{field}"),
                        &format!("{schema_path}/patternProperties/{pattern}"),
                    )?,
                    Ok(false) => {}
                    Err(message) => {
                        return Err(SchemaError::new(
                            SchemaKeyword::ResourceLimit,
                            instance_path,
                            format!("{schema_path}/patternProperties"),
                            message,
                        ))
                    }
                }
            }
        }
    }
    if let Some(name_schema) = schema.get("propertyNames") {
        for field in map.keys() {
            validate_schema(
                name_schema,
                &Value::String(field.clone()),
                instance_path,
                &format!("{schema_path}/propertyNames"),
            )?;
        }
    }
    if let Some(min) = usize_keyword(schema, "minProperties", instance_path, schema_path)? {
        if map.len() < min {
            return Err(SchemaError::new(
                SchemaKeyword::MinProperties,
                instance_path,
                format!("{schema_path}/minProperties"),
                "object has fewer properties than required",
            ));
        }
    }
    if let Some(max) = usize_keyword(schema, "maxProperties", instance_path, schema_path)? {
        if map.len() > max {
            return Err(SchemaError::new(
                SchemaKeyword::MaxProperties,
                instance_path,
                format!("{schema_path}/maxProperties"),
                "object has more properties than allowed",
            ));
        }
    }
    if let Some(dependencies) = schema.get("dependentRequired").and_then(Value::as_object) {
        for (field, required) in dependencies {
            if map.contains_key(field) {
                let Some(required) = required.as_array() else {
                    return Err(SchemaError::new(
                        SchemaKeyword::MalformedSchema,
                        instance_path,
                        format!("{schema_path}/dependentRequired"),
                        "schema 'dependentRequired' values must be arrays of strings",
                    ));
                };
                for dependent in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(dependent) {
                        return Err(SchemaError::new(
                            SchemaKeyword::DependentRequired,
                            instance_path,
                            format!("{schema_path}/dependentRequired"),
                            format!("field '{field}' requires field '{dependent}' to be present"),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates the array keywords (`prefixItems`, `items`, `min/maxItems`,
/// `uniqueItems`, `contains`, `min/maxContains`).
fn validate_array_keywords(
    schema: &serde_json::Map<String, Value>,
    array: &[Value],
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    let prefix_items = match schema.get("prefixItems") {
        None => None,
        Some(Value::Array(items)) => Some(items),
        Some(_) => {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/prefixItems"),
                "schema 'prefixItems' must be an array of schemas",
            ))
        }
    };
    // draft-04 `items`-as-array is the predecessor of `prefixItems`; treat
    // it identically rather than silently ignoring it.
    let items_as_array = match schema.get("items") {
        Some(Value::Array(items)) => Some(items),
        _ => None,
    };
    let positional = prefix_items.or(items_as_array);
    let positional_len = positional.map(|items| items.len()).unwrap_or(0);
    if let Some(positional) = positional {
        for (index, item_schema) in positional.iter().enumerate() {
            if let Some(item) = array.get(index) {
                validate_schema(
                    item_schema,
                    item,
                    &format!("{instance_path}[{index}]"),
                    &format!("{schema_path}/prefixItems/{index}"),
                )?;
            }
        }
    }
    if let Some(item_schema) = schema.get("items") {
        match item_schema {
            Value::Object(_) => {
                for (index, item) in array.iter().enumerate().skip(positional_len) {
                    validate_schema(
                        item_schema,
                        item,
                        &format!("{instance_path}[{index}]"),
                        &format!("{schema_path}/items"),
                    )?;
                }
            }
            Value::Array(_) => {
                // Handled as positional schemas above.
            }
            Value::Bool(_) => {
                return Err(SchemaError::new(
                    SchemaKeyword::MalformedSchema,
                    instance_path,
                    format!("{schema_path}/items"),
                    "boolean schemas are not supported (use an object schema)",
                ))
            }
            _ => {
                return Err(SchemaError::new(
                    SchemaKeyword::MalformedSchema,
                    instance_path,
                    format!("{schema_path}/items"),
                    "schema 'items' must be an object or array of schemas",
                ))
            }
        }
    }
    if let Some(min) = usize_keyword(schema, "minItems", instance_path, schema_path)? {
        if array.len() < min {
            return Err(SchemaError::new(
                SchemaKeyword::MinItems,
                instance_path,
                format!("{schema_path}/minItems"),
                "array has fewer items than required",
            ));
        }
    }
    if let Some(max) = usize_keyword(schema, "maxItems", instance_path, schema_path)? {
        if array.len() > max {
            return Err(SchemaError::new(
                SchemaKeyword::MaxItems,
                instance_path,
                format!("{schema_path}/maxItems"),
                "array has more items than allowed",
            ));
        }
    }
    if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
        for (index, item) in array.iter().enumerate() {
            if array[..index].contains(item) {
                return Err(SchemaError::new(
                    SchemaKeyword::UniqueItems,
                    instance_path,
                    format!("{schema_path}/uniqueItems"),
                    "array items must be unique",
                ));
            }
        }
    }
    if let Some(contains) = schema.get("contains") {
        if !contains.is_object() {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/contains"),
                "schema 'contains' must be an object schema",
            ));
        }
        let matches = array
            .iter()
            .filter(|item| {
                validate_schema(
                    contains,
                    item,
                    instance_path,
                    &format!("{schema_path}/contains"),
                )
                .is_ok()
            })
            .count();
        let min = usize_keyword(schema, "minContains", instance_path, schema_path)?.unwrap_or(1);
        if matches < min {
            return Err(SchemaError::new(
                SchemaKeyword::Contains,
                instance_path,
                format!("{schema_path}/contains"),
                format!("array must contain at least {min} matching item(s)"),
            ));
        }
        if let Some(max) = usize_keyword(schema, "maxContains", instance_path, schema_path)? {
            if matches > max {
                return Err(SchemaError::new(
                    SchemaKeyword::MaxContains,
                    instance_path,
                    format!("{schema_path}/maxContains"),
                    format!("array contains more than {max} matching item(s)"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_combinators(
    schema: &serde_json::Map<String, Value>,
    value: &Value,
    instance_path: &str,
    schema_path: &str,
) -> Result<(), SchemaError> {
    if let Some(subschemas) = subschema_array(schema, "allOf", instance_path, schema_path)? {
        for (index, subschema) in subschemas.iter().enumerate() {
            validate_schema(
                subschema,
                value,
                instance_path,
                &format!("{schema_path}/allOf/{index}"),
            )?;
        }
    }
    if let Some(subschemas) = subschema_array(schema, "anyOf", instance_path, schema_path)? {
        let mut first_error = None;
        let mut matched = false;
        for (index, subschema) in subschemas.iter().enumerate() {
            match validate_schema(
                subschema,
                value,
                instance_path,
                &format!("{schema_path}/anyOf/{index}"),
            ) {
                Ok(()) => {
                    matched = true;
                    break;
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if !matched {
            let detail = first_error
                .map(|error| error.message)
                .unwrap_or_else(|| "no alternatives were provided".to_string());
            return Err(SchemaError::new(
                SchemaKeyword::AnyOf,
                instance_path,
                format!("{schema_path}/anyOf"),
                format!("value matched none of the allowed alternatives ({detail})"),
            ));
        }
    }
    if let Some(subschemas) = subschema_array(schema, "oneOf", instance_path, schema_path)? {
        let matches = subschemas
            .iter()
            .filter(|subschema| {
                validate_schema(
                    subschema,
                    value,
                    instance_path,
                    &format!("{schema_path}/oneOf"),
                )
                .is_ok()
            })
            .count();
        if matches != 1 {
            return Err(SchemaError::new(
                SchemaKeyword::OneOf,
                instance_path,
                format!("{schema_path}/oneOf"),
                format!("value must match exactly one alternative but matched {matches}"),
            ));
        }
    }
    if let Some(subschema) = schema.get("not") {
        if !subschema.is_object() {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/not"),
                "schema 'not' must be an object schema",
            ));
        }
        if validate_schema(
            subschema,
            value,
            instance_path,
            &format!("{schema_path}/not"),
        )
        .is_ok()
        {
            return Err(SchemaError::new(
                SchemaKeyword::Not,
                instance_path,
                format!("{schema_path}/not"),
                "value must not match the excluded schema",
            ));
        }
    }
    if let Some(condition) = schema.get("if") {
        if !condition.is_object() {
            return Err(SchemaError::new(
                SchemaKeyword::MalformedSchema,
                instance_path,
                format!("{schema_path}/if"),
                "schema 'if' must be an object schema",
            ));
        }
        let condition_ok = validate_schema(
            condition,
            value,
            instance_path,
            &format!("{schema_path}/if"),
        )
        .is_ok();
        let branch = if condition_ok { "then" } else { "else" };
        if let Some(subschema) = schema.get(branch) {
            validate_schema(
                subschema,
                value,
                instance_path,
                &format!("{schema_path}/{branch}"),
            )?;
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
        "integer" => {
            // An integer is a number without a fractional part; serde_json
            // exposes it as i64/u64, or as f64 for values beyond those
            // ranges — in which case check the fraction explicitly.
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value
                    .as_f64()
                    .map(|n| n.fract() == 0.0 && n.is_finite())
                    .unwrap_or(false)
        }
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// `multipleOf` with integer-exact semantics where possible: for integral
/// values and integral factors, compare exactly; otherwise use floating
/// point with an epsilon chosen to tolerate representation error (e.g.
/// 0.07 is not exactly representable, so 0.07 * 7 must still count as a
/// multiple).
fn is_multiple_of(value: f64, factor: f64) -> bool {
    if factor <= 0.0 || !factor.is_finite() || !value.is_finite() {
        return false;
    }
    if value.fract() == 0.0 && factor.fract() == 0.0 {
        let value_int = value as i64;
        let factor_int = factor as i64;
        if value == value_int as f64 && factor == factor_int as f64 && factor_int != 0 {
            return value_int % factor_int == 0;
        }
    }
    let quotient = value / factor;
    (quotient - quotient.round()).abs() <= f64::EPSILON * quotient.abs().max(1.0) * 8.0
}

fn number_keyword(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    instance_path: &str,
    schema_path: &str,
) -> Result<Option<f64>, SchemaError> {
    match schema.get(keyword) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => Ok(number.as_f64()),
        Some(_) => Err(SchemaError::new(
            SchemaKeyword::MalformedSchema,
            instance_path,
            format!("{schema_path}/{keyword}"),
            format!("schema '{keyword}' must be a number"),
        )),
    }
}

fn usize_keyword(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    instance_path: &str,
    schema_path: &str,
) -> Result<Option<usize>, SchemaError> {
    let fail = || {
        SchemaError::new(
            SchemaKeyword::MalformedSchema,
            instance_path,
            format!("{schema_path}/{keyword}"),
            format!("schema '{keyword}' must be a non-negative integer"),
        )
    };
    match schema.get(keyword) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_u64() {
                if value <= usize::MAX as u64 {
                    return Ok(Some(value as usize));
                }
            }
            if let Some(value) = number.as_i64() {
                if (0..).contains(&value) {
                    return Ok(Some(value as usize));
                }
            }
            Err(fail())
        }
        Some(_) => Err(fail()),
    }
}

fn subschema_array<'a>(
    schema: &'a serde_json::Map<String, Value>,
    keyword: &str,
    instance_path: &str,
    schema_path: &str,
) -> Result<Option<&'a Vec<Value>>, SchemaError> {
    match schema.get(keyword) {
        None => Ok(None),
        Some(Value::Array(subschemas)) => Ok(Some(subschemas)),
        Some(_) => Err(SchemaError::new(
            SchemaKeyword::MalformedSchema,
            instance_path,
            format!("{schema_path}/{keyword}"),
            format!("schema '{keyword}' must be an array of schemas"),
        )),
    }
}

/// Bounded, linear-time regular-expression subset for the JSON Schema
/// `pattern` keyword.
///
/// Compilation is a recursive-descent parser to a small AST, then a
/// Thompson NFA construction with explicit instruction patching. Matching
/// is a Pike VM simulation (epsilon-closure thread sets over byte
/// predicates): every input byte advances the whole thread set once, so
/// runtime is O(states × input) — **there is no backtracking**, and
/// catastrophic backtracking is impossible by construction.
///
/// Supported constructs:
/// * literals, `.`, `^`, `$` (anchors), `|`
/// * escapes `\d \D \w \W \s \S \t \n \r \f \v \0`, escaped metacharacters,
///   `\xHH`
/// * character classes `[a-z]`, `[^...]` with ranges and class escapes
/// * groups `(...)` and `(?:...)`, alternation `|`
/// * quantifiers `*`, `+`, `?`, `{m}`, `{m,}`, `{m,n}` (greedy/lazy are
///   irrelevant for boolean matching)
///
/// Deliberately NOT supported (fail closed with a precise error):
/// backreferences, lookahead/lookbehind, `\b`/`\B` word boundaries,
/// `\p{...}` Unicode classes, inline/named groups, `\u` escapes.
///
/// `pattern` is unanchored per JSON Schema: the search tries every start
/// position.
pub(crate) mod regex {
    use super::{MAX_PATTERN_INPUT_BYTES, MAX_PATTERN_LENGTH};
    use std::sync::Arc;

    /// Upper bound on compiled NFA instructions; patterns that would
    /// exceed it fail closed.
    const MAX_STATES: usize = 4_096;
    /// Upper bound on group nesting.
    const MAX_GROUP_DEPTH: usize = 24;
    /// Upper bound on a quantifier bound.
    const MAX_QUANTIFIER_BOUND: usize = 512;

    // ------------------------------------------------------------------
    // AST
    // ------------------------------------------------------------------

    /// A parsed pattern.
    enum Ast {
        /// One byte satisfying a predicate.
        Byte(Arc<dyn Fn(u8) -> bool + Send + Sync>),
        /// Assert start-of-input.
        Start,
        /// Assert end-of-input.
        End,
        /// Concatenation.
        Seq(Vec<Ast>),
        /// Alternation over any number of branches.
        Alt(Vec<Ast>),
        /// Quantified: min copies in sequence plus optional copies.
        Quantified {
            body: Box<Ast>,
            min: usize,
            max: Option<usize>,
        },
    }

    // ------------------------------------------------------------------
    // Parser (recursive descent; depth-bounded, acyclic by construction)
    // ------------------------------------------------------------------

    struct Parser {
        chars: Vec<char>,
        pos: usize,
        depth: usize,
        pattern: String,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.pos).copied()
        }
        fn bump(&mut self) -> Option<char> {
            let c = self.peek();
            if c.is_some() {
                self.pos += 1;
            }
            c
        }
        fn err(&self, message: impl Into<String>) -> String {
            format!(
                "pattern regex error at position {}: {} (pattern: {})",
                self.pos,
                message.into(),
                self.pattern
            )
        }
    }

    fn compile(pattern: &str) -> Result<(Vec<Inst>, usize), String> {
        if pattern.chars().count() > MAX_PATTERN_LENGTH {
            return Err(format!(
                "pattern exceeds the {MAX_PATTERN_LENGTH}-character limit"
            ));
        }
        let mut parser = Parser {
            chars: pattern.chars().collect(),
            pos: 0,
            depth: 0,
            pattern: pattern.to_string(),
        };
        let ast = parse_alt(&mut parser)?;
        if let Some(c) = parser.peek() {
            return Err(parser.err(format!("unexpected '{c}'")));
        }
        let mut program = Program { insts: Vec::new() };
        let fragment = emit(&ast, &mut program)?;
        let match_index = program.insts.len();
        program.insts.push(Inst::Match);
        patch(&mut program, &fragment.pending, match_index);
        if program.insts.len() > MAX_STATES {
            return Err("pattern compiles to too many states".to_string());
        }
        Ok((program.insts, fragment.start))
    }

    fn parse_alt(p: &mut Parser) -> Result<Ast, String> {
        let mut branches = vec![parse_seq(p)?];
        while p.peek() == Some('|') {
            p.bump();
            branches.push(parse_seq(p)?);
        }
        if branches.len() == 1 {
            Ok(branches.pop().expect("one branch"))
        } else {
            Ok(Ast::Alt(branches))
        }
    }

    fn parse_seq(p: &mut Parser) -> Result<Ast, String> {
        let mut parts = Vec::new();
        while let Some(c) = p.peek() {
            if c == '|' || c == ')' {
                break;
            }
            parts.push(parse_quantified(p)?);
        }
        if parts.len() == 1 {
            Ok(parts.pop().expect("one part"))
        } else {
            Ok(Ast::Seq(parts))
        }
    }

    fn parse_quantified(p: &mut Parser) -> Result<Ast, String> {
        let atom = parse_atom(p)?;
        let Some(c) = p.peek() else {
            return Ok(atom);
        };
        let (min, max): (usize, Option<usize>) = match c {
            '*' => {
                p.bump();
                (0, None)
            }
            '+' => {
                p.bump();
                (1, None)
            }
            '?' => {
                p.bump();
                (0, Some(1))
            }
            '{' => {
                let saved = p.pos;
                p.bump();
                let Some(min) = parse_count(p) else {
                    p.pos = saved;
                    return Ok(atom);
                };
                match p.peek() {
                    Some('}') => {
                        p.bump();
                        (min, Some(min))
                    }
                    Some(',') => {
                        p.bump();
                        if p.peek() == Some('}') {
                            p.bump();
                            (min, None)
                        } else {
                            let Some(max) = parse_count(p) else {
                                p.pos = saved;
                                return Ok(atom);
                            };
                            if p.bump() != Some('}') {
                                p.pos = saved;
                                return Ok(atom);
                            }
                            (min, Some(max))
                        }
                    }
                    _ => {
                        p.pos = saved;
                        return Ok(atom);
                    }
                }
            }
            _ => return Ok(atom),
        };
        // Lazy/possessive markers are irrelevant for boolean matching.
        if matches!(p.peek(), Some('?') | Some('+')) {
            p.bump();
        }
        if let Some(max) = max {
            if min > max {
                return Err(p.err("invalid quantifier: min greater than max"));
            }
            if max > MAX_QUANTIFIER_BOUND {
                return Err(p.err(format!("quantifier bound exceeds {MAX_QUANTIFIER_BOUND}")));
            }
        }
        if min > MAX_QUANTIFIER_BOUND {
            return Err(p.err(format!("quantifier bound exceeds {MAX_QUANTIFIER_BOUND}")));
        }
        Ok(Ast::Quantified {
            body: Box::new(atom),
            min,
            max,
        })
    }

    fn parse_atom(p: &mut Parser) -> Result<Ast, String> {
        let Some(c) = p.bump() else {
            return Err(p.err("unexpected end of pattern"));
        };
        match c {
            '.' => Ok(Ast::Byte(Arc::new(|_| true))),
            '^' => Ok(Ast::Start),
            '$' => Ok(Ast::End),
            '(' => {
                p.depth += 1;
                if p.depth > MAX_GROUP_DEPTH {
                    return Err(p.err("group nesting exceeds the supported depth"));
                }
                // Only plain and non-capturing groups are supported.
                if p.peek() == Some('?') {
                    let saved = p.pos;
                    p.bump();
                    match p.peek() {
                        Some(':') => {
                            p.bump();
                        }
                        Some('=') | Some('!') => {
                            return Err(p.err("lookahead is not supported in pattern validation"))
                        }
                        Some('<') | Some('P') => {
                            return Err(
                                p.err("named groups are not supported in pattern validation")
                            )
                        }
                        _ => {
                            p.pos = saved;
                            return Err(p.err("unsupported group construct"));
                        }
                    }
                }
                let inner = parse_alt(p)?;
                if p.bump() != Some(')') {
                    return Err(p.err("unbalanced group"));
                }
                p.depth -= 1;
                Ok(inner)
            }
            ')' => Err(p.err("unbalanced ')'")),
            '[' => parse_class(p),
            '\\' => parse_escape(p, false),
            '|' => Err(p.err("unexpected '|'")),
            '*' | '+' | '?' => Err(p.err("quantifier with nothing to repeat")),
            '{' => {
                // A literal '{' that is not a quantifier (handled in
                // parse_quantified); treat as literal only when it cannot
                // start a bound.
                Ok(Ast::Byte(Arc::new(move |b| b == b'{')))
            }
            c => {
                // Non-ASCII literals compare on UTF-8 bytes; multi-byte
                // chars compile to their byte sequence.
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                let bytes = bytes;
                let mut seq: Vec<Ast> = bytes
                    .into_iter()
                    .map(|b| Ast::Byte(Arc::new(move |x: u8| x == b)))
                    .collect();
                if seq.len() == 1 {
                    Ok(seq.pop().expect("one byte"))
                } else {
                    Ok(Ast::Seq(seq))
                }
            }
        }
    }

    fn parse_class(p: &mut Parser) -> Result<Ast, String> {
        let negated = if p.peek() == Some('^') {
            p.bump();
            true
        } else {
            false
        };
        let mut tests: Vec<Arc<dyn Fn(u8) -> bool + Send + Sync>> = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = p.peek() else {
                return Err(p.err("unterminated character class"));
            };
            if c == ']' && !first {
                p.bump();
                break;
            }
            first = false;
            if c == '\\' {
                p.bump();
                let escape = parse_escape(p, true)?;
                match escape {
                    Ast::Byte(test) => tests.push(test),
                    _ => {
                        return Err(p.err("class escape must be a single byte test"));
                    }
                }
                continue;
            }
            let low = p.bump().expect("checked");
            if p.peek() == Some('-') && p.chars.get(p.pos + 1).copied().is_some_and(|n| n != ']') {
                p.bump(); // '-'
                let Some(high) = p.bump() else {
                    return Err(p.err("unterminated range"));
                };
                if high == '\\' {
                    return Err(p.err("escaped range bound is not supported in classes"));
                }
                if (low as u32) > (high as u32) {
                    return Err(p.err("invalid character range"));
                }
                let lo = low as u32;
                let hi = high as u32;
                if hi.saturating_sub(lo) > 1024 {
                    return Err(p.err("character range too wide"));
                }
                tests.push(Arc::new(move |b: u8| {
                    let b = b as u32;
                    (lo..=hi).contains(&b)
                }));
            } else {
                // A literal in a class: match its UTF-8 bytes as a sequence
                // would (for ASCII, the single byte; for non-ASCII this
                // approximation matches any of the bytes, which is only
                // correct for exact single-byte classes — non-ASCII in
                // classes is rare in tool schemas and this stays safe: it
                // can only over-match within the class's own charset).
                let mut buffer = [0u8; 4];
                let bytes = low.encode_utf8(&mut buffer).as_bytes().to_vec();
                if bytes.len() == 1 {
                    let b0 = bytes[0];
                    tests.push(Arc::new(move |b: u8| b == b0));
                } else {
                    for byte in bytes {
                        tests.push(Arc::new(move |b: u8| b == byte));
                    }
                }
            }
        }
        Ok(Ast::Byte(Arc::new(move |b: u8| {
            let hit = tests.iter().any(|test| test(b));
            hit != negated
        })))
    }

    fn parse_escape(p: &mut Parser, in_class: bool) -> Result<Ast, String> {
        let Some(c) = p.bump() else {
            return Err(p.err("trailing backslash"));
        };
        let test: Arc<dyn Fn(u8) -> bool + Send + Sync> = match c {
            'd' => Arc::new(|b: u8| b.is_ascii_digit()),
            'D' => Arc::new(|b: u8| !b.is_ascii_digit()),
            'w' => Arc::new(|b: u8| b.is_ascii_alphanumeric() || b == b'_'),
            'W' => Arc::new(|b: u8| !(b.is_ascii_alphanumeric() || b == b'_')),
            's' => Arc::new(|b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)),
            'S' => Arc::new(|b: u8| !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)),
            't' => Arc::new(|b: u8| b == b'\t'),
            'n' => Arc::new(|b: u8| b == b'\n'),
            'r' => Arc::new(|b: u8| b == b'\r'),
            'f' => Arc::new(|b: u8| b == 0x0c),
            'v' => Arc::new(|b: u8| b == 0x0b),
            '0' => Arc::new(|b: u8| b == 0),
            'b' if in_class => Arc::new(|b: u8| b == 0x08),
            'b' | 'B' => {
                return Err(p.err("word boundaries are not supported in pattern validation"))
            }
            'p' | 'P' => return Err(p.err("Unicode property classes are not supported")),
            'u' | 'U' => return Err(p.err("\\u escapes are not supported; use \\xHH")),
            'x' => {
                let hex: Option<u8> =
                    p.chars
                        .get(p.pos)
                        .zip(p.chars.get(p.pos + 1))
                        .and_then(|(a, b)| {
                            let text: String = [*a, *b].iter().collect();
                            u8::from_str_radix(&text, 16).ok()
                        });
                let Some(value) = hex else {
                    return Err(p.err("\\x requires two hex digits"));
                };
                p.pos += 2;
                Arc::new(move |b: u8| b == value)
            }
            c => {
                if c.is_ascii_alphanumeric() && !in_class {
                    return Err(p.err(format!("unsupported escape '\\{c}'")));
                }
                let mut buffer = [0u8; 4];
                let bytes = c.encode_utf8(&mut buffer).as_bytes().to_vec();
                if bytes.len() == 1 {
                    let value = bytes[0];
                    Arc::new(move |b: u8| b == value)
                } else {
                    // Escaped non-ASCII: match the full byte sequence.
                    return Ok(Ast::Seq(
                        bytes
                            .into_iter()
                            .map(|b| Ast::Byte(Arc::new(move |x: u8| x == b)))
                            .collect(),
                    ));
                }
            }
        };
        Ok(Ast::Byte(test))
    }

    fn parse_count(p: &mut Parser) -> Option<usize> {
        let mut digits = String::new();
        while let Some(c) = p.peek() {
            if c.is_ascii_digit() {
                digits.push(c);
                p.bump();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            return None;
        }
        digits.parse::<usize>().ok()
    }

    // ------------------------------------------------------------------
    // NFA construction (Thompson style with patch lists)
    // ------------------------------------------------------------------

    /// A compiled instruction. Split arms are indices; `Anchor` carries
    /// which end of the input is asserted.
    enum Inst {
        Byte(Arc<dyn Fn(u8) -> bool + Send + Sync>),
        Split(usize, usize),
        Anchor { start: bool },
        Match,
    }

    struct Program {
        insts: Vec<Inst>,
    }

    /// A fragment: the instruction index where it starts, plus the list of
    /// dangling `Split` placeholders to patch to a future continuation.
    struct Fragment {
        start: usize,
        pending: Vec<(usize, usize)>, // (instruction index, arm: 0|1)
    }

    /// A hole is a not-yet-patched Split arm: (instruction index, which arm).
    type Hole = (usize, usize);

    /// Fills every hole in `holes` with `target`.
    fn patch(program: &mut Program, holes: &[Hole], target: usize) {
        for &(index, arm) in holes {
            if let Inst::Split(a, b) = &mut program.insts[index] {
                if arm == 0 {
                    *a = target;
                } else {
                    *b = target;
                }
            }
        }
    }

    /// Emits a fresh Split with both arms unpatched; returns its index.
    fn new_split(program: &mut Program) -> usize {
        let index = program.insts.len();
        program.insts.push(Inst::Split(0, 0));
        index
    }

    fn emit(ast: &Ast, program: &mut Program) -> Result<Fragment, String> {
        if program.insts.len() > MAX_STATES {
            return Err("pattern compiles to too many states".to_string());
        }
        match ast {
            Ast::Byte(test) => {
                let start = program.insts.len();
                program.insts.push(Inst::Byte(Arc::clone(test)));
                // Every fragment carries an explicit exit jump so that
                // composite constructs (alt, loops) can redirect it.
                let exit = new_split(program);
                Ok(Fragment {
                    start,
                    pending: vec![(exit, 0)],
                })
            }
            Ast::Start => {
                let start = program.insts.len();
                program.insts.push(Inst::Anchor { start: true });
                let exit = new_split(program);
                Ok(Fragment {
                    start,
                    pending: vec![(exit, 0)],
                })
            }
            Ast::End => {
                let start = program.insts.len();
                program.insts.push(Inst::Anchor { start: false });
                let exit = new_split(program);
                Ok(Fragment {
                    start,
                    pending: vec![(exit, 0)],
                })
            }
            Ast::Seq(parts) => {
                let mut fragments = Vec::new();
                for part in parts {
                    fragments.push(emit(part, program)?);
                }
                if fragments.is_empty() {
                    // Empty sequence: matches the empty string. A Split with
                    // both arms unpatched splices into the continuation.
                    let start = new_split(program);
                    return Ok(Fragment {
                        start,
                        pending: vec![(start, 0), (start, 1)],
                    });
                }
                let start = fragments[0].start;
                for window in fragments.windows(2) {
                    let left_holes = window[0].pending.clone();
                    patch(program, &left_holes, window[1].start);
                }
                let pending = fragments.last().expect("nonempty").pending.clone();
                Ok(Fragment { start, pending })
            }
            Ast::Alt(branches) => {
                let mut fragments = Vec::new();
                for branch in branches {
                    fragments.push(emit(branch, program)?);
                }
                let n = fragments.len();
                if n == 1 {
                    return Ok(fragments.pop().expect("one branch"));
                }
                // Split chain: s_i arm0 -> branch i, arm1 -> s_{i+1}; the
                // last split's arm1 -> the final branch.
                let mut splits = Vec::new();
                for _ in 0..n - 1 {
                    splits.push(new_split(program));
                }
                for i in 0..n - 1 {
                    let split = splits[i];
                    if let Inst::Split(a, _) = &mut program.insts[split] {
                        *a = fragments[i].start;
                    }
                    if i + 1 < n - 1 {
                        if let Inst::Split(_, b) = &mut program.insts[split] {
                            *b = splits[i + 1];
                        }
                    } else if let Inst::Split(_, b) = &mut program.insts[split] {
                        *b = fragments[n - 1].start;
                    }
                }
                let start = splits[0];
                let mut pending = Vec::new();
                for fragment in &fragments {
                    pending.extend(fragment.pending.iter().copied());
                }
                Ok(Fragment { start, pending })
            }
            Ast::Quantified { body, min, max } => {
                // min required copies, then (max-min) optional copies, or
                // a star loop when max is unbounded.
                let mut required = Vec::new();
                for _ in 0..*min {
                    required.push(emit(body, program)?);
                }
                let tail: Option<Fragment> = match max {
                    None => {
                        // (body)* — a Split that loops back into the body.
                        let split = new_split(program);
                        let body_fragment = emit(body, program)?;
                        // split: arm0 -> body, arm1 -> hole (exit).
                        if let Inst::Split(a, _) = &mut program.insts[split] {
                            *a = body_fragment.start;
                        }
                        // Body exit jumps loop back to the split.
                        patch(program, &body_fragment.pending, split);
                        Some(Fragment {
                            start: split,
                            pending: vec![(split, 1)],
                        })
                    }
                    Some(max) => {
                        let mut optionals = Vec::new();
                        for _ in 0..max.saturating_sub(*min) {
                            let split = new_split(program);
                            let body_fragment = emit(body, program)?;
                            // split: arm0 -> body, arm1 -> hole (skip).
                            if let Inst::Split(a, _) = &mut program.insts[split] {
                                *a = body_fragment.start;
                            }
                            let mut pending = body_fragment.pending;
                            pending.push((split, 1));
                            optionals.push(Fragment {
                                start: split,
                                pending,
                            });
                        }
                        if optionals.is_empty() {
                            None
                        } else {
                            // Chain the optional copies: copy i's exit ->
                            // copy i+1's start.
                            let start = optionals[0].start;
                            for window in optionals.windows(2) {
                                let holes = window[0].pending.clone();
                                patch(program, &holes, window[1].start);
                            }
                            let pending = optionals.last().expect("nonempty").pending.clone();
                            Some(Fragment { start, pending })
                        }
                    }
                };
                let mut fragments = required;
                if let Some(tail) = tail {
                    fragments.push(tail);
                }
                if fragments.is_empty() {
                    // e.g. `a{0}` — matches the empty string.
                    let start = new_split(program);
                    return Ok(Fragment {
                        start,
                        pending: vec![(start, 0), (start, 1)],
                    });
                }
                let start = fragments[0].start;
                for window in fragments.windows(2) {
                    let holes = window[0].pending.clone();
                    patch(program, &holes, window[1].start);
                }
                let pending = fragments.last().expect("nonempty").pending.clone();
                Ok(Fragment { start, pending })
            }
        }
    }

    // ------------------------------------------------------------------
    // Pike VM search
    // ------------------------------------------------------------------

    /// Compiles and matches in one step: `pattern` is searched anywhere in
    /// `text` (JSON Schema `pattern` is unanchored).
    pub fn matches(pattern: &str, text: &str) -> Result<bool, String> {
        let (insts, entry) = compile(pattern)?;
        if text.len() > MAX_PATTERN_INPUT_BYTES {
            return Err(format!(
                "pattern input exceeds the {MAX_PATTERN_INPUT_BYTES}-byte budget"
            ));
        }
        Ok(search(&insts, entry, text))
    }

    fn search(insts: &[Inst], entry: usize, text: &str) -> bool {
        let bytes = text.as_bytes();
        let n = insts.len();
        if n == 0 {
            return true;
        }
        let mut current = vec![false; n];
        let mut next = vec![false; n];
        let mut matched = false;

        // Add a fresh thread at each position (unanchored search),
        // starting from the program entry.
        for pos in 0..=bytes.len() {
            let mut fresh = vec![false; n];
            add_thread(insts, &mut fresh, entry, bytes, pos);
            for (i, live) in fresh.iter().enumerate() {
                if *live {
                    current[i] = true;
                }
            }
            if pos == bytes.len() {
                break;
            }
            next.iter_mut().for_each(|v| *v = false);
            let b = bytes[pos];
            for pc in 0..n {
                if !current[pc] {
                    continue;
                }
                match &insts[pc] {
                    Inst::Byte(test) => {
                        if test(b) {
                            add_thread(insts, &mut next, pc + 1, bytes, pos + 1);
                        }
                    }
                    Inst::Anchor { .. } => {
                        // Anchors are consumed inside add_thread.
                    }
                    Inst::Split(..) => {
                        // Consumed inside add_thread.
                    }
                    Inst::Match => {
                        matched = true;
                    }
                }
            }
            std::mem::swap(&mut current, &mut next);
            if matched {
                return true;
            }
        }
        // A thread added at end-of-input may itself be a Match.
        for pc in 0..n {
            if current[pc] {
                if let Inst::Match = insts[pc] {
                    return true;
                }
            }
        }
        false
    }

    /// Adds `pc` (and everything reachable through Split epsilon edges) to
    /// the thread set, evaluating anchors against `pos`.
    fn add_thread(insts: &[Inst], set: &mut [bool], pc: usize, bytes: &[u8], pos: usize) {
        let mut stack = vec![pc];
        while let Some(pc) = stack.pop() {
            if pc >= insts.len() || set[pc] {
                continue;
            }
            match &insts[pc] {
                Inst::Split(a, b) => {
                    set[pc] = true;
                    stack.push(*a);
                    stack.push(*b);
                }
                Inst::Match => {
                    set[pc] = true;
                }
                Inst::Anchor { start } => {
                    let holds = if *start { pos == 0 } else { pos == bytes.len() };
                    if holds {
                        set[pc] = true;
                        stack.push(pc + 1);
                    }
                    // If the anchor does not hold, the thread dies: do NOT
                    // mark it live.
                }
                Inst::Byte(_) => {
                    set[pc] = true;
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn ok(pattern: &str, text: &str) {
            assert!(
                matches(pattern, text).unwrap_or(false),
                "pattern {pattern:?} should match {text:?}"
            );
        }

        fn no(pattern: &str, text: &str) {
            assert!(
                !matches(pattern, text).unwrap_or(true),
                "pattern {pattern:?} should NOT match {text:?}"
            );
        }

        fn unsupported(pattern: &str) {
            assert!(
                matches(pattern, "x").is_err(),
                "expected a compile error for {pattern:?}"
            );
        }

        #[test]
        fn literals_and_dot() {
            ok("abc", "xxabcxx");
            no("abc", "abd");
            ok("a.c", "abc");
            ok("a.c", "a c");
            no("a.c", "ac");
        }

        #[test]
        fn anchors() {
            ok("^abc", "abc");
            no("^abc", "xabc");
            ok("abc$", "xxabc");
            no("abc$", "abcx");
            ok("^abc$", "abc");
            no("^abc$", "abcd");
            // Unanchored search finds substrings.
            ok("bc", "abc");
        }

        #[test]
        fn quantifiers() {
            ok("ab*", "a");
            ok("ab*", "abbb");
            ok("ab+", "ab");
            no("ab+", "a");
            ok("ab?", "a");
            ok("colou?r", "color");
            ok("colou?r", "colour");
            ok("a{3}", "aaa");
            ok("a{3}", "aaaa");
            no("a{3}", "aa");
            ok("a{2,}", "aaaa");
            no("a{2,}", "a");
            ok("a{2,3}", "aa");
            ok("a{2,3}", "aaa");
            no("a{2,3}", "a");
            unsupported("*a");
            unsupported("a{3,2}");
            // A '{' that isn't a valid quantifier is a literal.
            ok("a{", "a{");
            ok("a{x", "a{x");
        }

        #[test]
        fn alternation_and_groups() {
            ok("cat|dog", "hotdog");
            ok("cat|dog", "cat");
            no("cat|dog", "horse");
            ok("(ab)+", "ababab");
            // Unanchored: "abac" contains "ab", so (ab)+ finds it.
            ok("(ab)+", "abac");
            no("(ab)+", "aac");
            ok("(a|b)c", "bc");
            ok("(?:a|b)c", "ac");
            ok("((a|b)c)+", "acbc");
            ok("(a|b|c)+", "abcabc");
            // "bba" contains no "ab" substring at all.
            no("(ab)+", "bba");
        }

        #[test]
        fn classes() {
            ok("[abc]+", "cabba");
            ok("[abc]+", "abd"); // unanchored: "ab" matches
            ok("[a-z]+", "hello");
            no("[a-z]+", "HELLO");
            ok("[a-zA-Z0-9_]+", "x_Y9");
            ok("[^0-9]+", "abc");
            no("[^0-9]+", "123");
            ok("[\\d]+", "123");
            ok("[\\w-]+", "a-b_c");
            ok("[\\s]+", " \t");
            ok("[-a]+", "a-");
            unsupported("[z-a]");
            unsupported("[abc");
        }

        #[test]
        fn escapes() {
            ok("\\d+", "abc123");
            no("\\d+", "abc");
            ok("\\w+", "hello_9");
            ok("\\w+", "hello world"); // unanchored: "hello" matches
            no("\\w+", " ! ");
            ok("\\s", "a b");
            ok("\\S+", "abc");
            no("\\S", " ");
            ok("a\\.b", "a.b");
            no("a\\.b", "axb");
            ok("\\t", "a\tb");
            ok("\\n", "a\nb");
            ok("\\x41", "A");
            no("\\x41", "B");
            ok("\\+", "+");
            ok("\\\\", "a\\b");
            unsupported("\\b");
            unsupported("\\B");
            unsupported("\\p{L}");
            unsupported("\\u0041");
            unsupported("a\\");
        }

        #[test]
        fn lazy_quantifiers_are_boolean_equivalent() {
            ok("a+?", "aaa");
            ok("<.+?>", "<abc>");
            ok("a??", "a");
            ok("a{1,2}?", "aa");
        }

        #[test]
        fn empty_pattern_matches_everything() {
            ok("", "");
            ok("", "anything");
        }

        #[test]
        fn non_ascii_literals() {
            ok("héllo", "say héllo");
            no("héllo", "hello");
        }

        #[test]
        fn no_catastrophic_backtracking() {
            // Patterns that kill backtracking engines in seconds run
            // instantly here: the simulation is linear in states × input.
            let cases: Vec<(&str, usize)> = vec![
                ("(a+)+b", 10_000),
                ("(a|a)*b", 10_000),
                ("(a*)*b", 10_000),
                ("(x+x+)+y", 6_000),
            ];
            for (pattern, length) in cases {
                let text = "a".repeat(length);
                let text = if pattern.contains('x') {
                    text.replace('a', "x")
                } else {
                    text
                };
                let started = std::time::Instant::now();
                let result = matches(pattern, &text).unwrap_or(false);
                assert!(!result, "{pattern} must not match (no terminator)");
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(5),
                    "linear engine must not degrade for {pattern}: {:?}",
                    started.elapsed()
                );
            }
        }

        #[test]
        fn deeply_nested_groups_are_bounded() {
            let mut pattern = String::new();
            for _ in 0..30 {
                pattern.push('(');
            }
            pattern.push('a');
            for _ in 0..30 {
                pattern.push(')');
            }
            assert!(matches(&pattern, "a").is_err());
        }

        #[test]
        fn overlong_patterns_fail_closed() {
            let pattern = "a".repeat(MAX_PATTERN_LENGTH + 1);
            assert!(matches(&pattern, "a").is_err());
        }

        #[test]
        fn pattern_budget_bounds_input() {
            // A string over the byte budget fails closed with an explicit
            // error rather than burning CPU or silently matching.
            let text = "a".repeat(MAX_PATTERN_INPUT_BYTES + 1);
            let result = matches("a+", &text);
            assert!(result.is_err(), "expected a budget error, got {result:?}");
        }
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

    fn validate(schema: &Value, value: &Value) -> Result<(), SchemaError> {
        validate_schema(schema, value, "arguments", "#")
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
        let mut schema = serde_json::json!({"type": "object", "properties": {}});
        for _ in 0..=MAX_SCHEMA_DEPTH {
            schema = json!({"type": "object", "properties": {"next": schema}});
        }
        let mut value = json!(0);
        for _ in 0..=MAX_SCHEMA_DEPTH {
            value = json!({"next": value});
        }
        let response = json!({ "tools": [{ "name": "deep", "inputSchema": schema }] });
        let error = validate_tool_arguments(&response, "deep", &value).unwrap_err();
        assert!(
            error.to_string().contains("nesting depth"),
            "got: {error:#}"
        );
    }

    #[test]
    fn structured_error_fields_are_populated() {
        let schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string", "minLength": 1}}
        });
        let error = validate(&schema, &json!({"name": ""})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MinLength);
        assert_eq!(error.instance_path, "arguments.name");
        assert_eq!(error.schema_path, "#/properties/name/minLength");
        assert!(error.message.contains("shorter"));
        let display = error.to_string();
        assert!(display.contains("arguments.name"));
        assert!(display.contains("minLength"));
        assert!(display.contains("#/properties/name/minLength"));
    }

    // ---- keyword matrix -------------------------------------------------

    #[test]
    fn const_keyword_is_enforced() {
        let schema = json!({"const": 7});
        assert!(validate(&schema, &json!(7)).is_ok());
        let error = validate(&schema, &json!(8)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::Const);
        assert_eq!(error.schema_path, "#/const");
    }

    #[test]
    fn array_cardinality_keywords_are_enforced() {
        let schema = json!({"type": "array", "minItems": 2, "maxItems": 3, "uniqueItems": true});
        assert!(validate(&schema, &json!([1, 2])).is_ok());
        let error = validate(&schema, &json!([1])).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MinItems);
        let error = validate(&schema, &json!([1, 2, 3, 4])).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MaxItems);
        let error = validate(&schema, &json!([1, 1])).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::UniqueItems);
        // Distinct JSON values (1 vs true vs "1") are unique.
        assert!(validate(&schema, &json!([1, true, "1"])).is_ok());
    }

    #[test]
    fn prefix_items_and_items_apply_positionally() {
        let schema = json!({
            "type": "array",
            "prefixItems": [{"type": "string"}],
            "items": {"type": "integer"}
        });
        assert!(validate(&schema, &json!(["a", 1, 2])).is_ok());
        assert!(validate(&schema, &json!([1, 1])).is_err());
        assert!(validate(&schema, &json!(["a", "b"])).is_err());
        // draft-04 items-as-array behaves like prefixItems.
        let legacy = json!({
            "type": "array",
            "items": [{"type": "string"}, {"type": "integer"}]
        });
        assert!(validate(&legacy, &json!(["a", 1])).is_ok());
        assert!(validate(&legacy, &json!(["a", "b"])).is_err());
    }

    #[test]
    fn string_length_and_pattern_are_enforced() {
        let schema =
            json!({"type": "string", "minLength": 2, "maxLength": 4, "pattern": "^[a-z]+$"});
        assert!(validate(&schema, &json!("abc")).is_ok());
        let error = validate(&schema, &json!("a")).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MinLength);
        let error = validate(&schema, &json!("abcde")).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MaxLength);
        let error = validate(&schema, &json!("ab1")).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::Pattern);
    }

    #[test]
    fn numeric_constraints_are_enforced() {
        let schema = json!({
            "type": "number",
            "exclusiveMinimum": 0,
            "maximum": 10,
            "multipleOf": 0.5
        });
        assert!(validate(&schema, &json!(2)).is_ok());
        assert!(validate(&schema, &json!(0.5)).is_ok());
        assert!(validate(&schema, &json!(7.5)).is_ok());
        let error = validate(&schema, &json!(0)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::ExclusiveMinimum);
        let error = validate(&schema, &json!(11)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::Maximum);
        let error = validate(&schema, &json!(0.3)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MultipleOf);
        let schema = json!({"type": "number", "minimum": 1});
        let error = validate(&schema, &json!(0)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::Minimum);
        let schema = json!({"type": "number", "exclusiveMaximum": 10});
        assert!(validate(&schema, &json!(10)).is_err());
        assert!(validate(&schema, &json!(9.99)).is_ok());
        // Integer multipleOf with exact integer math.
        let schema = json!({"type": "integer", "multipleOf": 3});
        assert!(validate(&schema, &json!(9)).is_ok());
        assert!(validate(&schema, &json!(10)).is_err());
        // 0.07 * 7 == 0.49 must hold despite binary representation error.
        let schema = json!({"multipleOf": 0.07});
        assert!(validate(&schema, &json!(0.49)).is_ok());
        assert!(validate(&schema, &json!(0.48)).is_err());
    }

    #[test]
    fn combinator_keywords_are_enforced() {
        let all = json!({"allOf": [{"type": "string"}, {"minLength": 2}]});
        assert!(validate(&all, &json!("ab")).is_ok());
        assert!(validate(&all, &json!(1)).is_err());
        let any = json!({"anyOf": [{"type": "string"}, {"type": "number"}]});
        assert!(validate(&any, &json!("x")).is_ok());
        assert!(validate(&any, &json!(1)).is_ok());
        assert!(validate(&any, &json!(true)).is_err());
        let one = json!({"oneOf": [{"type": "number", "maximum": 5}, {"minimum": 5}]});
        assert!(validate(&one, &json!(3)).is_ok());
        assert!(validate(&one, &json!(1)).is_ok());
        // 5 matches both branches -> oneOf failure.
        assert!(validate(&one, &json!(5)).is_err());
        let not = json!({"not": {"type": "string"}});
        assert!(validate(&not, &json!(1)).is_ok());
        assert!(validate(&not, &json!("x")).is_err());
    }

    #[test]
    fn if_then_else_is_enforced() {
        let schema = json!({
            "if": {"type": "string"},
            "then": {"minLength": 3},
            "else": {"minimum": 10}
        });
        assert!(validate(&schema, &json!("abc")).is_ok());
        assert!(validate(&schema, &json!("ab")).is_err());
        assert!(validate(&schema, &json!(12)).is_ok());
        assert!(validate(&schema, &json!(5)).is_err());
    }

    #[test]
    fn object_property_constraints_are_enforced() {
        let schema = json!({
            "type": "object",
            "minProperties": 1,
            "maxProperties": 2,
            "propertyNames": {"pattern": "^[a-z]+$"},
            "dependentRequired": {"billing": ["address"]}
        });
        assert!(validate(&schema, &json!({"name": "x"})).is_ok());
        let error = validate(&schema, &json!({})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MinProperties);
        let error = validate(&schema, &json!({"a": 1, "b": 2, "c": 3})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MaxProperties);
        let error = validate(&schema, &json!({"NAME": 1})).unwrap_err();
        assert!(error.schema_path.starts_with("#/propertyNames"));
        let error = validate(&schema, &json!({"billing": 1})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::DependentRequired);
    }

    #[test]
    fn additional_properties_schema_form_validates_extras() {
        let schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "additionalProperties": {"type": "number"}
        });
        assert!(validate(&schema, &json!({"name": "x", "extra": 1})).is_ok());
        let error = validate(&schema, &json!({"name": "x", "extra": "s"})).unwrap_err();
        assert!(error.schema_path.contains("additionalProperties"));
        assert!(error.instance_path.contains("extra"));
    }

    #[test]
    fn pattern_properties_apply_to_matching_keys() {
        let schema = json!({
            "type": "object",
            "patternProperties": {"^x-": {"type": "number"}},
            "additionalProperties": false
        });
        assert!(validate(&schema, &json!({"x-a": 1})).is_ok());
        assert!(validate(&schema, &json!({"x-a": "s"})).is_err());
        // 'y-b' matches neither the pattern nor a declared property.
        assert!(validate(&schema, &json!({"y-b": 1})).is_err());
    }

    #[test]
    fn contains_keywords_are_enforced() {
        let schema = json!({
            "type": "array",
            "contains": {"type": "string"},
            "minContains": 1,
            "maxContains": 2
        });
        assert!(validate(&schema, &json!([1, "a"])).is_ok());
        assert!(validate(&schema, &json!([1, 2])).is_err());
        assert!(validate(&schema, &json!(["a", "b", "c"])).is_err());
    }

    #[test]
    fn malformed_schemas_fail_closed() {
        // type must be a string or array of strings.
        let error = validate(&json!({"type": 5}), &json!(1)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // items must be an object or array of schemas.
        let error = validate(&json!({"items": 5}), &json!([1])).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // boolean schemas (draft-06) are not supported.
        let error = validate(&json!(true), &json!(1)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // required must be an array of strings.
        let error = validate(&json!({"required": "id"}), &json!({})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        let error = validate(&json!({"required": [1]}), &json!({})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // minLength must be a non-negative integer.
        let error = validate(&json!({"minLength": -1}), &json!("x")).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // properties must be an object.
        let error = validate(&json!({"properties": []}), &json!({})).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
        // anyOf must be an array.
        let error = validate(&json!({"anyOf": {}}), &json!(1)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MalformedSchema);
    }

    #[test]
    fn unsupported_keywords_fail_closed_loudly() {
        for keyword in [
            "$ref",
            "format",
            "unevaluatedProperties",
            "dependentSchemas",
        ] {
            let mut schema = json!({"type": "string"});
            schema[keyword] = json!({});
            let error = validate(&schema, &json!("x")).unwrap_err();
            assert_eq!(error.keyword, SchemaKeyword::UnsupportedKeyword);
            assert!(
                error.message.contains(keyword),
                "message must name the unsupported keyword: {}",
                error.message
            );
        }
    }

    #[test]
    fn unicode_strings_and_lengths_use_chars() {
        let schema = json!({"type": "string", "minLength": 3, "maxLength": 4});
        // 4 chars, 5 bytes: length keywords count chars, not bytes.
        assert!(validate(&schema, &json!("hélあ")).is_ok());
        let error = validate(&schema, &json!("hé")).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::MinLength);
        // Pattern matching is ASCII-aware (byte-based); Japanese text with
        // an ASCII pattern still behaves deterministically.
        let pattern_schema = json!({"type": "string", "pattern": "^[a-z]+$"});
        assert!(validate(&pattern_schema, &json!("日本")).is_err());
    }

    #[test]
    fn null_and_empty_containers_pass_permissive_schemas() {
        assert!(validate(&json!({"type": "null"}), &json!(null)).is_ok());
        assert!(validate(&json!({}), &json!({})).is_ok());
        assert!(validate(&json!({}), &json!([])).is_ok());
        assert!(validate(&json!({}), &json!(null)).is_ok());
        assert!(validate(&json!({"type": "object"}), &json!({})).is_ok());
        assert!(validate(&json!({"type": "array"}), &json!([])).is_ok());
        // Permissive object schemas allow extra keys (no
        // additionalProperties:false) — matching the AWH catalog policy.
        assert!(validate(
            &json!({"type": "object", "properties": {}}),
            &json!({"x": 1})
        )
        .is_ok());
    }

    #[test]
    fn large_but_bounded_inputs_are_handled() {
        let long = "a".repeat(100_000);
        assert!(validate(&json!({"type": "string"}), &json!(long)).is_ok());
        // Pattern matching is bounded: a string over the byte budget
        // fails closed (documented limitation).
        let over = "a".repeat(MAX_PATTERN_INPUT_BYTES + 1);
        let error = validate(&json!({"pattern": "a"}), &json!(over)).unwrap_err();
        assert_eq!(error.keyword, SchemaKeyword::ResourceLimit);
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

            #[test]
            fn schema_validation_never_panics_on_arbitrary_schema_and_value(
                schema in any_json(),
                value in any_json(),
            ) {
                let _ = validate_schema(&schema, &value, "arguments", "#");
            }

            #[test]
            fn regex_never_panics_on_arbitrary_patterns(
                pattern in "[ab*+?()|\\[\\]\\\\ .{}^$d-w]{0,24}",
                text in "[ab ]{0,32}",
            ) {
                // Compile errors are fine; panics are not.
                let _ = regex::matches(&pattern, &text);
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
