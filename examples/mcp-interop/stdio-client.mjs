// MCP interoperability harness for Agent Workspace Hub (stdio transport).
// Uses the official @modelcontextprotocol/sdk reference client — the same
// protocol stack OpenCode and Codex use — to exercise a real end-to-end
// workflow: connect -> initialize -> tools/list -> tools/call -> shutdown.
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const AWH = process.argv[2] ?? "/workspace/project/Agent-workspace-hub/target/release/awh";
// Isolated HOME so the harness cannot pollute the operator's real
// ~/.agent-workspace-hub (memory, trust store, skills).
const HOME = "/tmp/awh-interop-home";
import { mkdirSync } from "node:fs";
mkdirSync(HOME, { recursive: true });

function fail(step, err) {
  console.error(`FAIL [${step}]: ${err.message ?? err}`);
  process.exit(1);
}

const client = new Client({ name: "awh-interop-client", version: "1.0.0" }, {
  capabilities: { roots: {}, sampling: {} },
});

// --- connect + initialize -----------------------------------------------
let sessionInfo;
try {
  const transport = new StdioClientTransport({
    command: AWH,
    args: ["mcp", "serve"],
    env: { ...process.env, HOME },
  });
  await client.connect(transport);
  sessionInfo = client.getServerVersion();
  console.log("PASS connect + initialize");
  console.log(`  server: ${sessionInfo.name} ${sessionInfo.version}`);
} catch (e) {
  fail("connect", e);
}

// --- tools/list ----------------------------------------------------------
let tools;
try {
  ({ tools } = await client.listTools());
  if (!Array.isArray(tools) || tools.length === 0) throw new Error("no tools advertised");
  const names = tools.map((t) => t.name).sort();
  console.log(`PASS tools/list (${tools.length} tools)`);
  console.log(`  ${names.join(", ")}`);
  for (const t of tools) {
    if (!t.inputSchema) throw new Error(`tool ${t.name} missing inputSchema`);
  }
  console.log("PASS every tool has an inputSchema");
} catch (e) {
  fail("tools/list", e);
}

// --- tools/call: workspace context --------------------------------------
try {
  const result = await client.callTool({ name: "workspace.context", arguments: {} });
  const text = (result.content ?? []).map((c) => c.text ?? "").join("");
  if (result.isError) throw new Error(`tool returned isError: ${text.slice(0, 200)}`);
  console.log("PASS tools/call workspace.context");
  console.log(`  ${(text ?? "").slice(0, 140).replace(/\s+/g, " ")}...`);
} catch (e) {
  fail("tools/call workspace.context", e);
}

// --- tools/call: skills.list --------------------------------------------
try {
  const result = await client.callTool({ name: "skills.list", arguments: {} });
  if (result.isError) throw new Error("skills.list returned isError");
  console.log("PASS tools/call skills.list");
} catch (e) {
  fail("tools/call skills.list", e);
}

// --- tools/call: memory store -> search end-to-end -----------------------
try {
  const id = `interop-${Date.now()}`;
  const store = await client.callTool({
    name: "memory.store",
    arguments: {
      id,
      content: "written by the official MCP SDK client",
      scope: "Project",
      tags: ["interop"],
    },
  });
  if (store.isError) throw new Error("memory.store returned isError");
  const search = await client.callTool({
    name: "memory.search",
    arguments: { query: "SDK client" },
  });
  const text = (search.content ?? []).map((c) => c.text ?? "").join("");
  if (!text.includes(id)) {
    throw new Error(`search result missing stored id: ${text.slice(0, 200)}`);
  }
  console.log("PASS tools/call memory.store -> memory.search round-trip");
} catch (e) {
  fail("memory round-trip", e);
}

// --- tools/call: unknown tool must be a clean protocol error -------------
try {
  const result = await client.callTool({ name: "no.such.tool", arguments: {} });
  // MCP spec: unknown tool is either a JSON-RPC error response or a tool
  // result with isError=true. Both are protocol-correct; a crash/hang is not.
  if (result.isError) {
    console.log("PASS unknown tool -> isError tool result (protocol-correct)");
  } else {
    console.log("FAIL unknown tool was treated as success");
    process.exit(1);
  }
} catch (e) {
  // The SDK surfaces a JSON-RPC error response as a thrown McpError; that is
  // the spec-preferred behavior for unknown tools.
  if (e.code !== undefined) {
    console.log(`PASS unknown tool -> JSON-RPC error (code ${e.code})`);
  } else {
    fail("unknown tool", e);
  }
}

// --- session lifecycle: clean shutdown ----------------------------------
try {
  await client.close();
  console.log("PASS clean disconnect (client.close)");
} catch (e) {
  fail("disconnect", e);
}

console.log("STDIO INTEROP: ALL CHECKS PASSED");
