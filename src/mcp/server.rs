use crate::mcp::SkillMcp;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

pub struct StdioMcpServer {
    skills: SkillMcp,
}

impl StdioMcpServer {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        Ok(Self { skills: SkillMcp::new(project_root)? })
    }

    pub fn handle(&self, input: &str) -> Result<String> {
        let req: RpcRequest = serde_json::from_str(input)?;
        let result = match req.method.as_str() {
            "initialize" => json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"agent-workspace-hub","version":env!("CARGO_PKG_VERSION")}}),
            "tools/list" => json!({"tools":[
                {"name":"skills.list","description":"List skills referenced by the current project","inputSchema":{"type":"object","properties":{}}},
                {"name":"skills.read","description":"Read a project-referenced skill","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
                {"name":"skills.add","description":"Add an installed global skill to the current project","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
                {"name":"skills.remove","description":"Remove a skill reference from the current project","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
                {"name":"skills.search","description":"Search globally installed skills","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}
            ]}),
            "tools/call" => self.call_tool(&req.params)?,
            _ => json!({"error":"method not found"}),
        };
        Ok(serde_json::to_string(&RpcResponse { jsonrpc: "2.0", id: req.id, result: Some(result), error: None })?)
    }

    fn call_tool(&self, params: &Value) -> Result<Value> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let value = match name {
            "skills.list" => serde_json::to_value(self.skills.list()?)?,
            "skills.read" => serde_json::to_value(self.skills.read(args.get("name").and_then(Value::as_str).unwrap_or_default())?)?,
            "skills.add" => { self.skills.add(args.get("name").and_then(Value::as_str).unwrap_or_default())?; json!({"ok":true}) }
            "skills.remove" => json!({"removed": self.skills.remove(args.get("name").and_then(Value::as_str).unwrap_or_default())?}),
            "skills.search" => serde_json::to_value(self.skills.search_global(args.get("query").and_then(Value::as_str).unwrap_or_default())?)?,
            _ => json!({"error":"unknown tool"}),
        };
        Ok(json!({"content":[{"type":"text","text":serde_json::to_string(&value)?}]}))
    }
}
