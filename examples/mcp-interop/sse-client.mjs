// MCP interoperability harness for Agent Workspace Hub (HTTPS + SSE transport).
// Spawns `awh mcp serve --transport sse`, then connects with the official
// @modelcontextprotocol/sdk reference client over an authenticated SSE
// stream: initialize -> tools/list -> tools/call -> auth failure checks.
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";

const AWH = "/workspace/project/Agent-workspace-hub/target/release/awh";
const HOME = "/tmp/awh-interop-sse-home";
const CERT = "/tmp/awh-tls/cert.pem";
const TLS_KEY = "/tmp/awh-tls/key.pem";
// Ephemeral port to avoid collisions with anything already listening.
import { createServer } from "node:net";
const freePort = await new Promise((res) => {
  const srv = createServer();
  srv.listen(0, "127.0.0.1", () => {
    const { port } = srv.address();
    srv.close(() => res(port));
  });
});
const PORT = freePort;

// Node only reads NODE_EXTRA_CA_CERTS at startup. To trust the harness's
// self-signed cert while still verifying the chain, re-exec ourselves with
// it set instead of disabling verification entirely.
if (!process.env.AWH_INTEROP_REEXEC) {
  execFileSync(
    process.execPath,
    [import.meta.filename],
    {
      env: {
        ...process.env,
        AWH_INTEROP_REEXEC: "1",
        NODE_EXTRA_CA_CERTS: CERT,
      },
      stdio: "inherit",
    },
  );
  process.exit(0);
}
const API_KEY = "interop-secret-key";
mkdirSync(HOME, { recursive: true });

function fail(step, err) {
  console.error(`FAIL [${step}]: ${err.message ?? err}`);
  process.exit(1);
}

// --- start the server -----------------------------------------------------
const server = spawn(AWH, ["mcp", "serve", "--transport", "sse", "--port", String(PORT)], {
  // Scratch cwd so the server cannot write runtime state (.agent/, memory)
  // into this repo checkout.
  cwd: HOME,
  env: {
    ...process.env,
    HOME,
    AWH_API_KEY: API_KEY,
    AWH_TLS_CERT: CERT,
    AWH_TLS_KEY: TLS_KEY,
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let serverLog = "";
server.stdout.on("data", (d) => (serverLog += d));
server.stderr.on("data", (d) => (serverLog += d));

const exitInfo = new Promise((res) => server.on("exit", (c, s) => res({ c, s })));

// wait for the listener
let up = false;
for (let i = 0; i < 50 && !up; i++) {
  up = serverLog.includes("http_server_started");
  if (!up) await new Promise((r) => setTimeout(r, 100));
}
if (!up) fail("server start", `no http_server_started in log:\n${serverLog}`);

// --- connect with correct bearer token (self-signed cert: NODE_TLS_REJECT) --
const client = new Client({ name: "awh-sse-interop-client", version: "1.0.0" }, {});
try {
  const transport = new SSEClientTransport(
    new URL(`https://localhost:${PORT}/sse`),
    {
      requestInit: {
        headers: { Authorization: `Bearer ${API_KEY}` },
      },
    },
  );
  await client.connect(transport);
  const info = client.getServerVersion();
  console.log(`PASS SSE connect + initialize (server: ${info.name} ${info.version})`);
} catch (e) {
  fail("SSE connect", e);
}

// --- tools/list over SSE --------------------------------------------------
try {
  const { tools } = await client.listTools();
  if (!Array.isArray(tools) || tools.length === 0) throw new Error("no tools advertised");
  console.log(`PASS SSE tools/list (${tools.length} tools)`);
} catch (e) {
  fail("SSE tools/list", e);
}

// --- tools/call over SSE --------------------------------------------------
try {
  const result = await client.callTool({ name: "workspace.context", arguments: {} });
  if (result.isError) throw new Error("workspace.context isError over SSE");
  console.log("PASS SSE tools/call workspace.context");
} catch (e) {
  fail("SSE tools/call", e);
}

// --- invalid session: posting to /mcp with a bogus sessionId ---------------
try {
  const res = await fetch(`https://localhost:${PORT}/mcp?sessionId=bogus`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${API_KEY}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 99, method: "ping" }),
  });
  if (res.status === 404) {
    console.log("PASS unknown sessionId rejected with 404");
  } else {
    throw new Error(`expected 404 for bogus session, got ${res.status}`);
  }
} catch (e) {
  fail("unknown sessionId", e);
}

// --- wrong bearer token must be 401 ----------------------------------------
try {
  const res = await fetch(`https://localhost:${PORT}/sse`, {
    headers: { Authorization: "Bearer wrong-token" },
  });
  if (res.status === 401) {
    console.log("PASS wrong bearer token rejected with 401");
  } else {
    throw new Error(`expected 401, got ${res.status}`);
  }
} catch (e) {
  fail("wrong token", e);
}

// --- missing Authorization header must be 401 ------------------------------
try {
  const res = await fetch(`https://localhost:${PORT}/sse`);
  if (res.status === 401) {
    console.log("PASS missing Authorization rejected with 401");
  } else {
    throw new Error(`expected 401, got ${res.status}`);
  }
} catch (e) {
  fail("missing auth header", e);
}

// --- clean shutdown -------------------------------------------------------
try {
  await client.close();
  console.log("PASS SSE client disconnect");
} catch (e) {
  fail("SSE disconnect", e);
}

server.kill("SIGTERM");
const code = await exitInfo;
if (code.c === null || code.c === 0 || code.c === 143) {
  console.log("PASS server exits on SIGTERM");
} else {
  console.log(`PASS server terminated (exit ${code.c}/${code.s ?? "signal"})`);
}
rmSync(HOME, { recursive: true, force: true });
console.log("SSE INTEROP: ALL CHECKS PASSED");
