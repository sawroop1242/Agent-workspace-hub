//! TP11 — Skills boundary verification suite.
//!
//! Drives the REAL boundaries: the compiled `awh` binary across fresh
//! processes (CLI plane and MCP stdio plane), the canonical
//! `GlobalSkillRegistry` / `ProjectSkillReferences` / `SkillInstaller`
//! over real filesystem state, a real local HTTP registry server, and raw
//! persisted bytes as the independent oracle. Nothing is mocked and no
//! second registry/reference/package implementation is built here.
//!
//! Pre-existing coverage this suite does NOT duplicate: the unit tests in
//! `src/skills/{parser,package,registry,project,store,trust,lockfile,
//! registries,remote}.rs` and `src/mcp/skills.rs` (parse/validate,
//! add/remove/toggle, fail-closed enable/disable, atomic save, migration
//! of legacy reference files), and `tests/mcp_http.rs` (skills.* MCP
//! resource round-trips). This suite adds the boundary/black-box gaps.
//!
//! Sections (per docs/testing-prompts/11-skills.md):
//! - §5 manifest parsing at the real binary.
//! - §6/§14 package + filesystem safety incl. the D1/D2 traversal fix.
//! - §7 global registry behavior.
//! - §8/§9 project reference + enable/disable security semantics.
//! - §10 CLI black-box lifecycle.
//! - §11 MCP skills boundary.
//! - §13 Control API delegation.
//! - §15/§16 remote source + integrity.
//! - §17/§18/§19 transactionality, restart, corruption.
//! - §20/§25 concurrency, replay, lifecycle edge cases.
//! - §21/§22/§23 authorization, audit, sensitive data.

use agent_workspace_hub::skills::{GlobalSkillRegistry, ProjectSkillReferences, SkillInstaller};
use serde_json::{json, Value};
use sha2::Digest as _;
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use tempfile::TempDir;

// ---------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------

fn awh() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_awh"));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// A disposable environment: an isolated global skills root (via
/// `AWH_GLOBAL_SKILLS_ROOT`, never HOME mutation) and a project root.
struct Env {
    _dir: TempDir,
    global: PathBuf,
    project: PathBuf,
}

impl Env {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let global = dir.path().join("global-skills");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        Self {
            _dir: dir,
            global,
            project,
        }
    }

    fn registry(&self) -> GlobalSkillRegistry {
        GlobalSkillRegistry::new(self.global.clone())
    }

    fn references(&self) -> ProjectSkillReferences {
        ProjectSkillReferences::new(self.project.clone())
    }

    /// Runs one `awh` CLI invocation in the project, with the isolated
    /// global skills root injected per-child.
    fn run(&self, args: &[&str]) -> (bool, String, String) {
        self.run_in(&self.project, args)
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> (bool, String, String) {
        let mut cmd = awh();
        cmd.args(args)
            .current_dir(cwd)
            .env("AWH_GLOBAL_SKILLS_ROOT", &self.global);
        let mut child = cmd.spawn().unwrap();
        drop(child.stdin.take());
        let output = child.wait_with_output().unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// Installs a valid skill into the isolated global registry and
    /// returns its name.
    fn install(&self, name: &str) -> String {
        self.registry().create(name, "test skill").unwrap();
        name.to_string()
    }

    /// Adds a project reference to an installed skill (used by tests that
    /// seed state without driving the CLI).
    #[allow(dead_code)]
    fn reference(&self, name: &str) {
        self.references().add(name, &self.registry()).unwrap();
    }

    fn skills_json(&self) -> PathBuf {
        self.project.join(".agent").join("skills.json")
    }

    fn read_refs(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.skills_json()).unwrap())
            .expect("skills.json is valid JSON")
    }
}

/// A minimal HTTP registry server serving one manifest + skill package.
struct RegistryServer {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
}

impl RegistryServer {
    fn start(manifest: Value, package: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let manifest_body = manifest.to_string();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 4096];
                let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let (status, body) = if path == "/registry.json" {
                    ("200 OK", manifest_body.clone())
                } else if path == "/pkg/SKILL.md" {
                    ("200 OK", package.clone())
                } else {
                    ("404 Not Found", String::new())
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
            }
        });
        Self {
            port,
            _thread: handle,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

fn valid_manifest(name: &str, sha256: Option<String>) -> Value {
    json!({
        "name": "test-registry",
        "version": "1",
        "skills": [{
            "name": name,
            "description": "remote test skill",
            "version": "1.0.0",
            "path": "pkg/SKILL.md",
            "sha256": sha256,
        }]
    })
}

fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: remote test skill\nversion: 1.0.0\n---\n\n# {name}\n")
}

// ---------------------------------------------------------------------
// §5 manifest parsing at the real binary
// ---------------------------------------------------------------------

/// §5: the parser's real rules, exercised through installed global skills
/// the binary then lists/shows. Invalid metadata never becomes an
/// installed trusted skill.
#[test]
fn installed_skill_parsing_rules_hold_at_the_binary() {
    let env = Env::new();
    let reg = env.registry();

    // valid: name/description/version round-trip through `skill show`
    reg.create("valid-skill", "does a thing").unwrap();
    let (ok, out, _) = env.run(&["skill", "show", "valid-skill"]);
    assert!(ok);
    assert!(out.contains("name: valid-skill"));
    assert!(out.contains("description: does a thing"));
    assert!(out.contains("version: 0.1.0"));
    assert!(out.contains("installed: yes"));

    // malformed installed SKILL.md (missing front matter) fails to parse:
    // `skill show` errors, and `skill list` surfaces the failure rather
    // than silently dropping the directory.
    let broken = env.global.join("broken-skill");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("SKILL.md"), "no front matter\n").unwrap();
    let (ok, _, err) = env.run(&["skill", "show", "broken-skill"]);
    assert!(!ok, "malformed installed skill must not parse");
    assert!(err.contains("front matter"), "{err}");
    let (ok, _, err) = env.run(&["skill", "list"]);
    assert!(
        !ok,
        "a malformed installed skill must fail the listing, not vanish silently: {err}"
    );

    // a directory with no SKILL.md is ignored by list (not a skill)
    std::fs::create_dir_all(env.global.join("not-a-skill")).unwrap();
    std::fs::remove_dir_all(&broken).unwrap();
    let (ok, out, _) = env.run(&["skill", "list"]);
    assert!(ok);
    assert!(out.contains("valid-skill"));
    assert!(!out.contains("not-a-skill"));
}

// ---------------------------------------------------------------------
// §6/§14 package and filesystem safety (D1/D2 regression)
// ---------------------------------------------------------------------

/// §6/§14: `awh skill uninstall` must never address a directory outside
/// the global skills root. Pre-fix, `uninstall ../../victim` deleted an
/// arbitrary directory (D1). The name is now validated before the join.
#[test]
fn cli_uninstall_rejects_path_traversal_and_never_deletes_outside() {
    let env = Env::new();
    let _ = env.install("real-skill");

    // a victim directory one level above the skills root
    let victim = env._dir.path().join("victim");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("keep.txt"), "IMPORTANT").unwrap();

    for evil in [
        "../../victim",
        "../victim",
        "..",
        "/etc",
        "a/b",
        "a\\b",
        "with space",
        "UPPER",
    ] {
        let (ok, _, err) = env.run(&["skill", "uninstall", evil]);
        assert!(!ok, "uninstall {evil:?} must be rejected");
        assert!(err.contains("invalid skill name"), "{evil:?}: {err}");
    }
    // the victim and its contents are untouched
    assert!(victim.join("keep.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(victim.join("keep.txt")).unwrap(),
        "IMPORTANT"
    );
    // the legitimate skill is still installed
    assert!(env.global.join("real-skill").join("SKILL.md").is_file());
}

/// §6/§14: `awh skill install` must not let a registry-advertised (or
/// caller-supplied) name escape the skills root. Pre-fix, a manifest whose
/// entry name was `../../escape-target` deleted and overwrote a directory
/// outside the root (D2).
#[test]
fn cli_install_rejects_traversal_names_from_caller_and_registry() {
    let env = Env::new();
    let victim = env._dir.path().join("escape-target");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("keep.txt"), "IMPORTANT").unwrap();

    let server = RegistryServer::start(
        valid_manifest("../../escape-target", None),
        skill_md("escape-target"),
    );

    // caller-supplied traversal name
    let (ok, _, err) = env.run(&[
        "skill",
        "install",
        "../../escape-target",
        "--registry",
        &server.url(),
    ]);
    assert!(!ok, "traversal install must be rejected");
    assert!(err.contains("invalid skill name"), "{err}");
    assert!(victim.join("keep.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(victim.join("keep.txt")).unwrap(),
        "IMPORTANT"
    );
    assert!(!env.global.join("escape-target").exists());

    // a valid registry install still works (positive control)
    let server_ok =
        RegistryServer::start(valid_manifest("good-skill", None), skill_md("good-skill"));
    let (ok, out, err) = env.run(&[
        "skill",
        "install",
        "good-skill",
        "--registry",
        &server_ok.url(),
    ]);
    assert!(ok, "valid install must succeed: {err}");
    assert!(out.contains("installed global skill: good-skill"));
    assert!(env.global.join("good-skill").join("SKILL.md").is_file());
}

/// §6: a package with no SKILL.md, or an oversized SKILL.md, is rejected
/// before publication and leaves no partial install.
#[test]
fn package_validation_rejects_unsafe_layouts_without_partial_install() {
    let env = Env::new();
    let installer = SkillInstaller::new(env._dir.path().join("cache"));

    // no SKILL.md
    let empty = env._dir.path().join("empty-src");
    std::fs::create_dir_all(&empty).unwrap();
    let err = installer
        .install_from_local(&empty, &env.registry(), "empty-src")
        .unwrap_err();
    assert!(err.to_string().contains("SKILL.md"), "{err}");
    assert!(!env.global.join("empty-src").exists());

    // oversized SKILL.md (> 1 MiB)
    let big = env._dir.path().join("big-src");
    std::fs::create_dir_all(&big).unwrap();
    std::fs::write(big.join("SKILL.md"), "a".repeat(1024 * 1024 + 1)).unwrap();
    let err = installer
        .install_from_local(&big, &env.registry(), "big-src")
        .unwrap_err();
    assert!(err.to_string().contains("1 MiB"), "{err}");
    assert!(!env.global.join("big-src").exists());

    // install_from_local also validates the destination name
    let good = env._dir.path().join("good-src");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(
        good.join("SKILL.md"),
        "---\nname: good-src\ndescription: d\n---\n",
    )
    .unwrap();
    let err = installer
        .install_from_local(&good, &env.registry(), "../../escape")
        .unwrap_err();
    assert!(err.to_string().contains("invalid skill name"), "{err}");
    assert!(!env._dir.path().join("escape").exists());
}

// ---------------------------------------------------------------------
// §7 global registry behavior
// ---------------------------------------------------------------------

/// §7: list ordering is deterministic, missing is not fabricated, and
/// invalid names cannot address arbitrary paths.
#[test]
fn global_registry_listing_order_missing_and_name_safety() {
    let env = Env::new();
    for name in ["zeta", "alpha", "mid-skill"] {
        env.install(name);
    }
    // deterministic (sorted) order
    let names: Vec<String> = env
        .registry()
        .list()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["alpha", "mid-skill", "zeta"]);
    // repeated list is identical
    assert_eq!(env.registry().list().unwrap().len(), 3);

    // get missing → None (not fabricated); invalid names rejected
    assert!(env.registry().get("missing").unwrap().is_none());
    for evil in ["../x", "a/b", "UPPER", "", "with space"] {
        assert!(
            env.registry().get(evil).is_err(),
            "get {evil:?} must reject"
        );
    }

    // a global install does NOT activate in any project (installed ≠
    // referenced): the project reference file does not exist
    assert!(!env.skills_json().exists());
    let (ok, out, _) = env.run(&["skill", "project"]);
    assert!(ok);
    assert!(out.trim().is_empty(), "no project references yet: {out}");
}

/// §7: `create` rejects a duplicate and invalid names, and never writes
/// outside the root.
#[test]
fn global_registry_create_rejects_duplicates_and_bad_names() {
    let env = Env::new();
    let reg = env.registry();
    reg.create("dup", "first").unwrap();
    assert!(reg.create("dup", "second").is_err(), "duplicate rejected");
    for evil in ["", "UPPER", "with space", "a/b", "../evil"] {
        assert!(
            reg.create(evil, "d").is_err(),
            "create {evil:?} must reject"
        );
    }
    // only the one legitimate skill exists
    assert_eq!(reg.list().unwrap().len(), 1);
    assert!(!env._dir.path().join("evil").exists());
}

// ---------------------------------------------------------------------
// §8/§9 project references + enable/disable security semantics
// ---------------------------------------------------------------------

/// §8/§9: the four states stay distinct — installed ≠ referenced ≠
/// enabled ≠ authorized — and disable preserves the reference.
#[test]
fn reference_lifecycle_and_enable_disable_distinct_from_removal() {
    let env = Env::new();
    let _ = env.install("alpha");
    let _ = env.install("beta");

    // installed but unreferenced
    assert!(env.references().states().unwrap().is_empty());
    let (ok, out, _) = env.run(&["skill", "list"]);
    assert!(ok);
    assert!(out.contains("[installed; not referenced]"), "{out}");

    // reference alpha
    let (ok, out, _) = env.run(&["skill", "add", "alpha"]);
    assert!(ok);
    assert!(out.contains("added project skill reference: alpha"));
    let refs = env.read_refs();
    assert_eq!(refs["skills"], json!(["alpha"]));
    assert_eq!(refs["disabled"], json!([]));

    // duplicate add is not an error but does not duplicate
    let (ok, _, _) = env.run(&["skill", "add", "alpha"]);
    assert!(ok);
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // add of an uninstalled skill fails closed
    let (ok, _, err) = env.run(&["skill", "add", "ghost"]);
    assert!(!ok);
    assert!(err.contains("not installed globally"), "{err}");
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // disable: reference preserved, state recorded
    let (ok, out, _) = env.run(&["skill", "disable", "alpha"]);
    assert!(ok);
    assert!(out.contains("disabled project skill reference: alpha"));
    let refs = env.read_refs();
    assert_eq!(
        refs["skills"],
        json!(["alpha"]),
        "reference survives disable"
    );
    assert_eq!(refs["disabled"], json!(["alpha"]));
    let (ok, out, _) = env.run(&["skill", "list"]);
    assert!(ok);
    assert!(out.contains("[installed; referenced; disabled]"), "{out}");

    // re-add while disabled keeps it disabled (no silent state flip)
    let (ok, _, _) = env.run(&["skill", "add", "alpha"]);
    assert!(ok);
    assert_eq!(env.read_refs()["disabled"], json!(["alpha"]));

    // enable restores exposure
    let (ok, _, _) = env.run(&["skill", "enable", "alpha"]);
    assert!(ok);
    assert_eq!(env.read_refs()["disabled"], json!([]));

    // enable/disable of an unreferenced name fails closed, no mutation
    let (ok, _, err) = env.run(&["skill", "disable", "beta"]);
    assert!(!ok);
    assert!(err.contains("not referenced"), "{err}");
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // remove drops the reference (and any disable state), not the global
    let (ok, _, _) = env.run(&["skill", "disable", "alpha"]);
    assert!(ok);
    let (ok, out, _) = env.run(&["skill", "remove", "alpha"]);
    assert!(ok);
    assert!(out.contains("removed project skill reference"));
    assert_eq!(env.read_refs()["skills"], json!([]));
    assert_eq!(env.read_refs()["disabled"], json!([]));
    // the global install is untouched by a project-reference removal
    assert!(env.global.join("alpha").join("SKILL.md").is_file());

    // remove of a missing reference errors
    let (ok, _, err) = env.run(&["skill", "remove", "alpha"]);
    assert!(!ok);
    assert!(err.contains("not found"), "{err}");
}

/// §8: invalid names are rejected by every project-reference mutation and
/// never touch the filesystem outside the project.
#[test]
fn project_reference_rejects_invalid_names() {
    let env = Env::new();
    let refs = env.references();
    for evil in ["../evil", "a/b", "UPPER", "", "with space"] {
        assert!(
            refs.add(evil, &env.registry()).is_err(),
            "add {evil:?} must reject"
        );
        assert!(refs.enable(evil).is_err(), "enable {evil:?} must reject");
        assert!(refs.disable(evil).is_err(), "disable {evil:?} must reject");
    }
    assert!(!env.skills_json().exists());
}

/// §8/§18: a legacy `.agent/skills.json` without the `disabled` field
/// loads with every reference enabled (compatibility default).
#[test]
fn legacy_reference_file_defaults_all_enabled() {
    let env = Env::new();
    std::fs::create_dir_all(env.project.join(".agent")).unwrap();
    std::fs::write(env.skills_json(), r#"{"skills":["alpha","beta"]}"#).unwrap();
    let states = env.references().states().unwrap();
    assert_eq!(states.len(), 2);
    assert!(states.iter().all(|s| s.enabled));
    // the reference store loads without rewriting the legacy file
    assert_eq!(
        std::fs::read_to_string(env.skills_json()).unwrap(),
        r#"{"skills":["alpha","beta"]}"#
    );
}

// ---------------------------------------------------------------------
// §11 MCP skills boundary
// ---------------------------------------------------------------------

struct McpServer {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl McpServer {
    fn start(env: &Env) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
            .args(["mcp", "serve", "--transport", "stdio"])
            .current_dir(&env.project)
            .env("AWH_GLOBAL_SKILLS_ROOT", &env.global)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn awh mcp stdio");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout"));
        let mut server = McpServer {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 0,
        };
        server.initialize();
        server
    }

    fn send(&mut self, value: &Value) {
        let line = serde_json::to_string(value).unwrap();
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).expect("read stdout");
            if n == 0 {
                panic!("server closed stdout without responding");
            }
            if line.trim().is_empty() {
                continue;
            }
            return serde_json::from_str(line.trim())
                .unwrap_or_else(|e| panic!("stdout is not JSON-RPC ({e}): {line}"));
        }
    }

    fn req(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id": self.next_id, "method": method, "params": params}));
        self.read()
    }

    fn initialize(&mut self) {
        let r = self.req(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tp11-skills", "version": "0.0"}
            }),
        );
        assert!(
            r["result"]["protocolVersion"].is_string(),
            "initialize failed: {r}"
        );
        self.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.req("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn tool_json(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.tool(name, arguments);
        assert!(
            response["error"].is_null(),
            "tool {name} errored: {response}"
        );
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("envelope");
        serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} payload not JSON ({e}): {text}"))
    }
}

/// §11: MCP list/read/add/enable/disable converge on the canonical
/// reference store, disabled reads fail closed, and unreferenced reads
/// fail closed — all at the real dispatcher boundary.
#[test]
fn mcp_skills_lifecycle_and_disabled_read_gating() {
    let env = Env::new();
    let _ = env.install("alpha");
    let _ = env.install("beta");
    let mut mcp = McpServer::start(&env);

    // nothing referenced yet
    let listed = mcp.tool_json("skills.list", json!({}));
    assert_eq!(listed, json!([]));

    // add alpha + beta through MCP
    mcp.tool_json("skills.add", json!({"name": "alpha"}));
    mcp.tool_json("skills.add", json!({"name": "beta"}));
    let listed = mcp.tool_json("skills.list", json!({}));
    let names: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["enabled"] == json!(true)));

    // read enabled works
    let skill = mcp.tool_json("skills.read", json!({"name": "alpha"}));
    assert_eq!(skill["name"], json!("alpha"));

    // disable alpha: still listed (management visibility), read fails closed
    mcp.tool_json("skills.disable", json!({"name": "alpha"}));
    let listed = mcp.tool_json("skills.list", json!({}));
    let alpha = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "alpha")
        .unwrap();
    assert_eq!(alpha["enabled"], json!(false));
    let resp = mcp.tool("skills.read", json!({"name": "alpha"}));
    assert!(!resp["error"].is_null(), "disabled read must fail: {resp}");
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("disabled"), "{msg}");

    // re-enable restores exposure
    mcp.tool_json("skills.enable", json!({"name": "alpha"}));
    assert_eq!(
        mcp.tool_json("skills.read", json!({"name": "alpha"}))["name"],
        json!("alpha")
    );

    // read of an unreferenced (but installed) skill fails closed
    let _ = env.install("gamma");
    let resp = mcp.tool("skills.read", json!({"name": "gamma"}));
    assert!(
        !resp["error"].is_null(),
        "unreferenced read must fail: {resp}"
    );

    // enable/disable of an unreferenced name fails closed
    assert!(!mcp.tool("skills.enable", json!({"name": "gamma"}))["error"].is_null());
    assert!(!mcp.tool("skills.disable", json!({"name": "gamma"}))["error"].is_null());

    // malformed args → schema error, no mutation
    let before = std::fs::read(env.skills_json()).unwrap();
    assert!(!mcp.tool("skills.add", json!({}))["error"].is_null());
    assert!(!mcp.tool("skills.add", json!({"name": 5}))["error"].is_null());
    assert_eq!(std::fs::read(env.skills_json()).unwrap(), before);

    // remove through MCP drops the reference; canonical file agrees
    let removed = mcp.tool_json("skills.remove", json!({"name": "beta"}));
    assert_eq!(removed["removed"], json!(true));
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // cross-workspace: a second project's MCP server sees nothing
    let env2 = Env::new();
    let mut mcp2 = McpServer::start(&env2);
    assert_eq!(mcp2.tool_json("skills.list", json!({})), json!([]));
}

/// §11: the MCP plane is the same authority as the CLI — a CLI mutation
/// is visible over MCP and vice versa, and no MCP-local store exists.
#[test]
fn mcp_and_cli_share_the_project_reference_authority() {
    let env = Env::new();
    let _ = env.install("alpha");
    // CLI adds; MCP sees it
    let (ok, _, _) = env.run(&["skill", "add", "alpha"]);
    assert!(ok);
    let mut mcp = McpServer::start(&env);
    let listed = mcp.tool_json("skills.list", json!({}));
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["name"], json!("alpha"));
    // MCP disables; CLI observes disabled
    mcp.tool_json("skills.disable", json!({"name": "alpha"}));
    let (ok, out, _) = env.run(&["skill", "list"]);
    assert!(ok);
    assert!(out.contains("[installed; referenced; disabled]"), "{out}");
    // the only project store is .agent/skills.json
    assert!(env.skills_json().is_file());
    let mut names = Vec::new();
    for entry in std::fs::read_dir(env.project.join(".agent")).unwrap() {
        names.push(entry.unwrap().file_name().to_string_lossy().into_owned());
    }
    let skill_files: Vec<&String> = names.iter().filter(|n| n.contains("skill")).collect();
    assert_eq!(
        skill_files,
        vec![&"skills.json".to_string()],
        "no MCP-local skill store"
    );
}

// ---------------------------------------------------------------------
// §15/§16 remote source + integrity
// ---------------------------------------------------------------------

/// §15/§16: a matching SHA-256 installs; a mismatch never becomes
/// installed state; a missing digest is allowed (nothing to verify).
#[test]
fn remote_install_verifies_integrity_before_publication() {
    let env = Env::new();
    let pkg = skill_md("remote-skill");
    let digest = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(pkg.as_bytes()))
    };

    // matching digest → installed
    let server = RegistryServer::start(
        valid_manifest("remote-skill", Some(digest.clone())),
        pkg.clone(),
    );
    let (ok, out, err) = env.run(&[
        "skill",
        "install",
        "remote-skill",
        "--registry",
        &server.url(),
    ]);
    assert!(ok, "matching digest must install: {err}");
    assert!(out.contains("installed global skill: remote-skill"));
    assert!(env.global.join("remote-skill").join("SKILL.md").is_file());
    // independent digest of the installed bytes matches
    let installed = std::fs::read(env.global.join("remote-skill").join("SKILL.md")).unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&installed)), digest);
    // mismatching digest → rejected, no install
    let env2 = Env::new();
    let bad = "f".repeat(64);
    let server2 = RegistryServer::start(
        valid_manifest("bad-digest", Some(bad)),
        skill_md("bad-digest"),
    );
    let (ok, _, err) = env2.run(&[
        "skill",
        "install",
        "bad-digest",
        "--registry",
        &server2.url(),
    ]);
    assert!(!ok, "digest mismatch must fail");
    assert!(err.contains("integrity check failed"), "{err}");
    assert!(!env2.global.join("bad-digest").exists());

    // missing digest → allowed (registry published none)
    let env3 = Env::new();
    let server3 = RegistryServer::start(valid_manifest("no-digest", None), skill_md("no-digest"));
    let (ok, _, err) = env3.run(&[
        "skill",
        "install",
        "no-digest",
        "--registry",
        &server3.url(),
    ]);
    assert!(ok, "missing digest is allowed: {err}");
    assert!(env3.global.join("no-digest").join("SKILL.md").is_file());

    // malformed digest (wrong length) → rejected
    let env4 = Env::new();
    let server4 = RegistryServer::start(
        valid_manifest("short-digest", Some("abc".into())),
        skill_md("short-digest"),
    );
    let (ok, _, err) = env4.run(&[
        "skill",
        "install",
        "short-digest",
        "--registry",
        &server4.url(),
    ]);
    assert!(!ok, "malformed digest must fail");
    assert!(err.contains("invalid SHA-256"), "{err}");
}

/// §15: registry errors (404 manifest, missing skill, unsafe path) fail
/// closed and never fall back to another source or leave partial state.
#[test]
fn remote_registry_errors_fail_closed_without_fallback() {
    let env = Env::new();

    // manifest not found (server 404s everything)
    let server = RegistryServer::start(json!({"unused": true}), String::new());
    let (ok, _, err) = env.run(&["skill", "install", "anything", "--registry", &server.url()]);
    assert!(!ok);
    assert!(
        err.contains("invalid registry.json") || err.contains("HTTP 404"),
        "{err}"
    );

    // skill absent from a valid manifest
    let server2 = RegistryServer::start(valid_manifest("present", None), skill_md("present"));
    let (ok, _, err) = env.run(&["skill", "install", "absent", "--registry", &server2.url()]);
    assert!(!ok);
    assert!(err.contains("skill not found in registry"), "{err}");

    // unsafe path in the manifest entry
    let mut manifest = valid_manifest("unsafe", None);
    manifest["skills"][0]["path"] = json!("../../etc/passwd");
    let server3 = RegistryServer::start(manifest, skill_md("unsafe"));
    let (ok, _, err) = env.run(&["skill", "install", "unsafe", "--registry", &server3.url()]);
    assert!(!ok);
    assert!(err.contains("unsafe skill path"), "{err}");

    // connection failure (no server)
    let (ok, _, err) = env.run(&["skill", "install", "x", "--registry", "http://127.0.0.1:1"]);
    assert!(!ok);
    assert!(err.contains("failed to contact skill registry"), "{err}");
    assert!(!env.global.join("x").exists());
}

// ---------------------------------------------------------------------
// §17/§18/§19 transactionality, restart, corruption
// ---------------------------------------------------------------------

/// §18: reference + disabled state survive a fresh process; a corrupt
/// reference file fails closed (never silently empty).
#[test]
fn reference_state_survives_restart_and_corruption_fails_closed() {
    let env = Env::new();
    let _ = env.install("alpha");
    let _ = env.install("beta");
    let (ok, _, _) = env.run(&["skill", "add", "alpha"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "add", "beta"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "disable", "beta"]);
    assert!(ok);

    // fresh process reads the same persisted state
    let (ok, out, _) = env.run(&["skill", "project"]);
    assert!(ok);
    assert!(out.contains("alpha") && out.contains("beta"));
    let refs = env.read_refs();
    assert_eq!(refs["skills"], json!(["alpha", "beta"]));
    assert_eq!(refs["disabled"], json!(["beta"]));

    // corrupt the reference file: management ops fail closed
    std::fs::write(env.skills_json(), "{ broken").unwrap();
    let (ok, _, err) = env.run(&["skill", "project"]);
    assert!(!ok, "corrupt references must fail closed");
    assert!(!err.is_empty());
    let (ok, _, _) = env.run(&["skill", "add", "alpha"]);
    assert!(!ok, "mutations on a corrupt store must fail");
    // corrupt bytes preserved, not silently replaced with empty
    assert_eq!(
        std::fs::read_to_string(env.skills_json()).unwrap(),
        "{ broken"
    );
}

// ---------------------------------------------------------------------
// §20/§25 concurrency + replay
// ---------------------------------------------------------------------

/// §20: concurrent CLI adders serialize on the store lock — every add
/// survives and the file stays well-formed with no duplicates.
#[test]
fn concurrent_reference_additions_keep_state_valid() {
    let env = Env::new();
    let names = ["s1", "s2", "s3", "s4", "s5", "s6"];
    for name in names {
        env.install(name);
    }
    let threads: Vec<_> = names
        .iter()
        .map(|name| {
            let global = env.global.clone();
            let project = env.project.clone();
            let name = name.to_string();
            std::thread::spawn(move || {
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_awh"));
                cmd.args(["skill", "add", &name])
                    .current_dir(&project)
                    .env("AWH_GLOBAL_SKILLS_ROOT", &global)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let status = cmd.status().unwrap();
                assert!(status.success(), "concurrent add {name} failed");
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let refs = env.read_refs();
    let skills = refs["skills"].as_array().unwrap();
    assert_eq!(skills.len(), names.len(), "no lost updates: {skills:?}");
    let unique: std::collections::HashSet<_> = skills.iter().collect();
    assert_eq!(unique.len(), names.len(), "no duplicate references");
}

/// §25: repeated lifecycle operations follow the documented contract —
/// re-adding is idempotent, removing twice errors, enable/disable after
/// remove fail closed, and a removed reference is not resurrected.
#[test]
fn replay_and_lifecycle_edge_cases() {
    let env = Env::new();
    let _ = env.install("alpha");

    // add twice: second is a no-op success, still one reference
    assert!(env.run(&["skill", "add", "alpha"]).0);
    assert!(env.run(&["skill", "add", "alpha"]).0);
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // disable twice: idempotent
    assert!(env.run(&["skill", "disable", "alpha"]).0);
    assert!(env.run(&["skill", "disable", "alpha"]).0);
    assert_eq!(env.read_refs()["disabled"], json!(["alpha"]));

    // enable twice: idempotent
    assert!(env.run(&["skill", "enable", "alpha"]).0);
    assert!(env.run(&["skill", "enable", "alpha"]).0);
    assert_eq!(env.read_refs()["disabled"], json!([]));

    // remove once succeeds, twice errors
    assert!(env.run(&["skill", "remove", "alpha"]).0);
    assert!(!env.run(&["skill", "remove", "alpha"]).0);
    // enable/disable after remove fail closed (no resurrection)
    assert!(!env.run(&["skill", "enable", "alpha"]).0);
    assert!(!env.run(&["skill", "disable", "alpha"]).0);
    assert_eq!(env.read_refs()["skills"], json!([]));
    // the global install still exists; re-referencing works
    assert!(env.global.join("alpha").join("SKILL.md").is_file());
    assert!(env.run(&["skill", "add", "alpha"]).0);
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));
}

// ---------------------------------------------------------------------
// §13 Control API delegation
// ---------------------------------------------------------------------

/// §13: the Control API delegates to the canonical reference store — an
/// API add is visible to the CLI and raw file, and a CLI add is visible
/// through the API. Auth still applies.
#[test]
fn control_api_skills_delegates_to_canonical_store() {
    use agent_workspace_hub::api::control::{build_router, ControlState};
    use std::sync::Arc;
    use tower::ServiceExt as _;

    let env = Env::new();
    let _ = env.install("alpha");
    let _ = env.install("beta");

    let state = {
        let mut s = ControlState::new(env.project.clone(), "tp11-key".into());
        s.global_skills_root = Some(env.global.clone());
        Arc::new(s)
    };
    let router = build_router(state);

    let request = |method: &str, uri: &str, body: Option<Value>| {
        let builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", "Bearer tp11-key");
        let req = match body {
            Some(json) => builder
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(json.to_string()))
                .unwrap(),
            None => builder
                .header("Content-Length", "0")
                .body(axum::body::Body::empty())
                .unwrap(),
        };
        let router = router.clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let response = router.oneshot(req).await.unwrap();
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (
                    status,
                    serde_json::from_slice::<Value>(&bytes).unwrap_or(json!({})),
                )
            })
    };

    // API add → CLI + raw file see it
    let (status, body) = request(
        "POST",
        "/api/v1/skills/project",
        Some(json!({"name": "alpha"})),
    );
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let (ok, out, _) = env.run(&["skill", "list"]);
    assert!(ok);
    assert!(out.contains("[installed; referenced; enabled]"), "{out}");
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // CLI add → API list sees it
    assert!(env.run(&["skill", "add", "beta"]).0);
    let (status, body) = request("GET", "/api/v1/skills/project", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let names: Vec<&str> = body["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha") && names.contains(&"beta"));

    // API add of an uninstalled skill → 400, no mutation
    let before = std::fs::read(env.skills_json()).unwrap();
    let (status, _) = request(
        "POST",
        "/api/v1/skills/project",
        Some(json!({"name": "ghost"})),
    );
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read(env.skills_json()).unwrap(), before);

    // API remove → CLI observes
    let (status, _) = request("DELETE", "/api/v1/skills/project?name=beta", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(env.read_refs()["skills"], json!(["alpha"]));

    // no bearer → 401
    let router2 = {
        let mut s = ControlState::new(env.project.clone(), "tp11-key".into());
        s.global_skills_root = Some(env.global.clone());
        build_router(Arc::new(s))
    };
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/skills/project")
        .header("Content-Length", "0")
        .body(axum::body::Body::empty())
        .unwrap();
    let status = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move { router2.oneshot(req).await.unwrap().status() });
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------
// §22/§23 audit + sensitive-data protection
// ---------------------------------------------------------------------

/// §22/§23: skill mutations are durably audited (action + name), while a
/// token-shaped canary in a skill description is never copied into the
/// audit log or CLI error output.
#[test]
fn skill_mutations_are_audited_without_leaking_content() {
    let env = Env::new();
    let canary = "sk-live-0123456789abcdefghijklmnopqrstuvwxyz";
    // a skill whose description carries a token-shaped canary
    env.registry()
        .create("canary-skill", &format!("desc {canary}"))
        .unwrap();

    let (ok, _, _) = env.run(&["skill", "add", "canary-skill"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "disable", "canary-skill"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "enable", "canary-skill"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "remove", "canary-skill"]);
    assert!(ok);
    let (ok, _, _) = env.run(&["skill", "uninstall", "canary-skill"]);
    assert!(ok);

    let audit = env.project.join(".agent").join("audit").join("audit.log");
    assert!(audit.is_file(), "skill mutations must be durably audited");
    let audit_bytes = std::fs::read_to_string(&audit).unwrap();
    for action in [
        "cli_skill_add",
        "cli_skill_disable",
        "cli_skill_enable",
        "cli_skill_remove",
        "cli_skill_uninstall",
    ] {
        assert!(
            audit_bytes.contains(action),
            "missing audit action {action}"
        );
    }
    // the canary description never reaches the audit log
    assert!(!audit_bytes.contains(canary), "audit leaked skill content");

    // a failed mutation leaves no false-positive success audit
    let (ok, _, _) = env.run(&["skill", "add", "never-installed"]);
    assert!(!ok);
    let audit_bytes = std::fs::read_to_string(&audit).unwrap();
    assert!(!audit_bytes.contains("never-installed"));
}
