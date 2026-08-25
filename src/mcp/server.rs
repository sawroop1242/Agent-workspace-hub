use crate::mcp::{MemoryMcp, MemoryScope, SkillMcp, TaskPriority, TaskStatus, TasksMcp, WorkspaceMcp};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RpcRequest { jsonrpc: String, id: Option<Value>, method: String, #[serde(default)] params: Value }
#[derive(Debug, Serialize)]
struct RpcResponse { jsonrpc: &'static str, id: Option<Value>, result: Option<Value>, error: Option<Value> }

pub struct StdioMcpServer { skills: SkillMcp, workspace: WorkspaceMcp, memory: MemoryMcp, tasks: TasksMcp }

impl StdioMcpServer {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        Ok(Self { skills: SkillMcp::new(project_root.clone())?, workspace: WorkspaceMcp::new(project_root.clone())?, memory: MemoryMcp::new(project_root.clone())?, tasks: TasksMcp::new(project_root)? })
    }

    pub fn handle(&self, input: &str) -> Result<String> {
        let req: RpcRequest = serde_json::from_str(input)?;
        let result = match req.method.as_str() {
            "initialize" => json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"agent-workspace-hub","version":env!("CARGO_PKG_VERSION")}}),
            "tools/list" => self.tools_list(),
            "tools/call" => self.call_tool(&req.params)?,
            _ => json!({"error":"method not found"}),
        };
        Ok(serde_json::to_string(&RpcResponse { jsonrpc: "2.0", id: req.id, result: Some(result), error: None })?)
    }

    fn tools_list(&self) -> Value {
        json!({"tools":[
            {"name":"skills.list","description":"List skills referenced by the current project","inputSchema":{"type":"object","properties":{}}},
            {"name":"skills.read","description":"Read a project-referenced skill","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.add","description":"Add an installed global skill to the current project","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.remove","description":"Remove a skill reference from the current project","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
            {"name":"skills.search","description":"Search globally installed skills","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
            {"name":"workspace.context","description":"Read project-level agent instructions and README context","inputSchema":{"type":"object","properties":{}}},
            {"name":"workspace.list_files","description":"List files in a project directory","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"workspace.read_file","description":"Read a UTF-8 project file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
            {"name":"memory.store","description":"Store or update project memory","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"content":{"type":"string"},"scope":{"type":"string","enum":["Session","Project","Global"]},"tags":{"type":"array","items":{"type":"string"}}},"required":["id","content","scope"]}},
            {"name":"memory.search","description":"Search project memory","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"scope":{"type":"string","enum":["Session","Project","Global"]}},"required":["query"]}},
            {"name":"memory.get","description":"Get a memory entry by id","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"memory.delete","description":"Delete a memory entry","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
            {"name":"tasks.create","description":"Create a project task","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"priority":{"type":"string","enum":["Low","Normal","High","Critical"]},"tags":{"type":"array","items":{"type":"string"}}},"required":["id","title","description"]}},
            {"name":"tasks.list","description":"List project tasks, optionally filtered by status","inputSchema":{"type":"object","properties":{"status":{"type":"string","enum":["Todo","InProgress","Blocked","Done"]}}}},
            {"name":"tasks.update","description":"Update task status, priority or assignee","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"status":{"type":"string","enum":["Todo","InProgress","Blocked","Done"]},"priority":{"type":"string","enum":["Low","Normal","High","Critical"]},"assignee":{"type":["string","null"]}},"required":["id"]}},
            {"name":"tasks.delete","description":"Delete a project task","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}
        ]})
    }

    fn call_tool(&self, params: &Value) -> Result<Value> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let value = match name {
            "skills.list" => serde_json::to_value(self.skills.list()?)?,
            "skills.read" => serde_json::to_value(self.skills.read(args.get("name").and_then(Value::as_str).unwrap_or_default())?)?,
            "skills.add" => { self.skills.add(args.get("name").and_then(Value::as_str).unwrap_or_default())?; json!({"ok":true}) },
            "skills.remove" => json!({"removed": self.skills.remove(args.get("name").and_then(Value::as_str).unwrap_or_default())?}),
            "skills.search" => serde_json::to_value(self.skills.search_global(args.get("query").and_then(Value::as_str).unwrap_or_default())?)?,
            "workspace.context" => serde_json::to_value(self.workspace.context()?)?,
            "workspace.list_files" => serde_json::to_value(self.workspace.list_files(args.get("path").and_then(Value::as_str).unwrap_or("."))?)?,
            "workspace.read_file" => serde_json::to_value(self.workspace.read_file(args.get("path").and_then(Value::as_str).unwrap_or_default())?)?,
            "memory.store" => { let scope = match args.get("scope").and_then(Value::as_str).unwrap_or("Project") { "Session" => MemoryScope::Session, "Global" => MemoryScope::Global, _ => MemoryScope::Project }; serde_json::to_value(self.memory.store(args.get("id").and_then(Value::as_str).unwrap_or_default().to_string(), args.get("content").and_then(Value::as_str).unwrap_or_default().to_string(), scope, args.get("tags").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default())?)? }
            "memory.search" => { let scope = args.get("scope").and_then(Value::as_str).map(|s| match s { "Session" => MemoryScope::Session, "Global" => MemoryScope::Global, _ => MemoryScope::Project }); serde_json::to_value(self.memory.search(args.get("query").and_then(Value::as_str).unwrap_or_default(), scope)?)? }
            "memory.get" => serde_json::to_value(self.memory.get(args.get("id").and_then(Value::as_str).unwrap_or_default())?)?,
            "memory.delete" => json!({"deleted": self.memory.delete(args.get("id").and_then(Value::as_str).unwrap_or_default())?}),
            "tasks.create" => { let priority = parse_priority(args.get("priority").and_then(Value::as_str)); serde_json::to_value(self.tasks.create(args.get("id").and_then(Value::as_str).unwrap_or_default().to_string(), args.get("title").and_then(Value::as_str).unwrap_or_default().to_string(), args.get("description").and_then(Value::as_str).unwrap_or_default().to_string(), priority, string_array(&args, "tags"))?)? }
            "tasks.list" => serde_json::to_value(self.tasks.list(args.get("status").and_then(Value::as_str).map(parse_status))?)?,
            "tasks.update" => { let status = args.get("status").and_then(Value::as_str).map(parse_status); let priority = args.get("priority").and_then(Value::as_str).map(parse_priority); let assignee = args.get("assignee").map(|v| v.as_str().map(str::to_string)); serde_json::to_value(self.tasks.update(args.get("id").and_then(Value::as_str).unwrap_or_default(), status, priority, assignee)?)? }
            "tasks.delete" => json!({"deleted": self.tasks.delete(args.get("id").and_then(Value::as_str).unwrap_or_default())?}),
            _ => json!({"error":"unknown tool"}),
        };
        Ok(json!({"content":[{"type":"text","text":serde_json::to_string(&value)?}]}))
    }
}

fn parse_status(value: &str) -> TaskStatus { match value { "InProgress" => TaskStatus::InProgress, "Blocked" => TaskStatus::Blocked, "Done" => TaskStatus::Done, _ => TaskStatus::Todo } }
fn parse_priority(value: Option<&str>) -> TaskPriority { match value.unwrap_or("Normal") { "Low" => TaskPriority::Low, "High" => TaskPriority::High, "Critical" => TaskPriority::Critical, _ => TaskPriority::Normal } }
fn string_array(args: &Value, key: &str) -> Vec<String> { args.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default() }
