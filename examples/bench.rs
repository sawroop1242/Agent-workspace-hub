//! Lightweight micro-benchmarks for the security-critical hot paths.
//!
//! Run with:
//! ```text
//! cargo run --release --example bench
//! ```
//!
//! These are not criterion-grade benchmarks; they measure end-to-end wall-clock
//! time for the operations that sit on every MCP tool invocation, so a
//! regression here would directly translate to added per-request latency.

use std::hint::black_box;
use std::time::Instant;

use agent_workspace_hub::mcp::schema::validate_tool_arguments;

fn bench<F: FnMut()>(name: &str, iters: u32, mut f: F) {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let per_iter = elapsed / iters;
    println!("{name:<40} {iters:>8} iters  {per_iter:?}/iter");
}

fn main() {
    // `validate_tool_arguments` takes the raw `tools/list` response.
    let tools_response = serde_json::json!({
        "tools": [
            {
                "name": "echo",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" },
                        "count": { "type": "integer" }
                    },
                    "required": ["message"]
                }
            }
        ]
    });

    let args = serde_json::json!({ "message": "hello", "count": 3 });

    // Argument validation is the per-call security gate.
    bench("schema.validate_tool_arguments", 100_000, || {
        let _ = validate_tool_arguments(black_box(&tools_response), "echo", black_box(&args));
    });

    // A rejected call exercises the failure path (no panic allowed).
    let bad_args = serde_json::json!({ "message": 42 });
    bench("schema.validate_tool_arguments (reject)", 100_000, || {
        let _ = validate_tool_arguments(black_box(&tools_response), "echo", black_box(&bad_args));
    });
}
