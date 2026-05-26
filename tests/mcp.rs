use assert_cmd::Command;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

struct TestContext {
    temp_dir: TempDir,
    workspace_path: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().to_path_buf();
        std::fs::create_dir(workspace_path.join("pyre")).unwrap();

        let context = Self {
            temp_dir,
            workspace_path,
        };
        let _ = context.temp_dir.path();
        context
    }

    fn run_command(&self, subcommand: &str) -> assert_cmd::Command {
        let mut cmd = Command::cargo_bin("pyre").unwrap();
        cmd.current_dir(&self.workspace_path);
        cmd.arg(subcommand);
        cmd
    }
}

fn write_schema(ctx: &TestContext) {
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
record User {
    id   Int    @id
    name String
    @public
}
        "#,
    )
    .unwrap();
}

fn framed_json(value: &serde_json::Value) -> String {
    let body = value.to_string();
    format!("Content-Length: {}\r\n\r\n{}", body.as_bytes().len(), body)
}

fn parse_framed_json(output: &[u8]) -> serde_json::Value {
    let raw = String::from_utf8_lossy(output);
    let (_headers, body) = raw
        .split_once("\r\n\r\n")
        .expect("expected MCP framed response");
    serde_json::from_str(body).expect("expected JSON response body")
}

fn call_mcp(ctx: &TestContext, request: serde_json::Value) -> serde_json::Value {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("pyre"))
        .arg("mcp")
        .current_dir(&ctx.workspace_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pyre mcp");

    {
        let stdin = child.stdin.as_mut().expect("expected child stdin");
        stdin
            .write_all(framed_json(&request).as_bytes())
            .expect("failed to write MCP request");
    }
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .expect("failed to wait for pyre mcp");
    assert!(
        output.status.success(),
        "pyre mcp failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    parse_framed_json(&output.stdout)
}

fn call_mcp_tool(ctx: &TestContext, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    let response = call_mcp(
        ctx,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    );

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("expected MCP text content");
    serde_json::from_str(text).expect("expected JSON tool result text")
}

fn call_mcp_tool_error(
    ctx: &TestContext,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call_mcp(
        ctx,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    )
}

#[test]
fn initialize_ping_and_tools_list() {
    let ctx = TestContext::new();

    let initialize = call_mcp(
        &ctx,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        }),
    );
    assert_eq!(initialize["result"]["serverInfo"]["name"], "pyre");
    assert!(initialize["result"]["capabilities"]["tools"].is_object());

    let ping = call_mcp(
        &ctx,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        }),
    );
    assert_eq!(ping["result"], json!({}));

    let tools = call_mcp(
        &ctx,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/list"
        }),
    );
    let tool_names = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert!(tool_names.contains(&"pyre_query"));
    assert!(tool_names.contains(&"pyre_db_status"));
    assert!(tool_names.contains(&"pyre_schema"));
}

#[test]
fn parse_error_and_notification_no_response() {
    let ctx = TestContext::new();

    let mut parse_error_child = StdCommand::new(assert_cmd::cargo::cargo_bin("pyre"))
        .arg("mcp")
        .current_dir(&ctx.workspace_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pyre mcp");
    parse_error_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"Content-Length: 1\r\n\r\n{")
        .unwrap();
    drop(parse_error_child.stdin.take());
    let parse_error_output = parse_error_child.wait_with_output().unwrap();
    let parse_error = parse_framed_json(&parse_error_output.stdout);
    assert_eq!(parse_error["error"]["code"], -32700);

    let mut notification_child = StdCommand::new(assert_cmd::cargo::cargo_bin("pyre"))
        .arg("mcp")
        .current_dir(&ctx.workspace_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pyre mcp");
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    notification_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(framed_json(&notification).as_bytes())
        .unwrap();
    drop(notification_child.stdin.take());
    let notification_output = notification_child.wait_with_output().unwrap();
    assert!(notification_output.stdout.is_empty());
}

#[test]
fn schema_and_check_tools() {
    let ctx = TestContext::new();
    write_schema(&ctx);

    let schema = call_mcp_tool(&ctx, "pyre_schema", json!({}));
    assert_eq!(schema["schemas"][0]["namespace"], "_default");
    assert!(schema["schemas"][0]["content"]
        .as_str()
        .unwrap()
        .contains("record User"));

    let check = call_mcp_tool(&ctx, "pyre_check", json!({}));
    assert_eq!(check["ok"], true);
    assert_eq!(check["stdout"], json!("[]\n"));
}

#[test]
fn db_status_reports_up_to_date_after_push() {
    let ctx = TestContext::new();
    write_schema(&ctx);

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    let status = call_mcp_tool(&ctx, "pyre_db_status", json!({ "database": ".yak/yak.db" }));

    assert_eq!(status["ok"], true);
    assert_eq!(status["accessible"], true);
    assert_eq!(status["status"], "up_to_date");
    assert_eq!(status["pendingMigrations"], json!([]));
}

#[test]
fn pyre_query_executes_selection_and_mutation() {
    let ctx = TestContext::new();
    write_schema(&ctx);

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    let insert = call_mcp_tool(
        &ctx,
        "pyre_query",
        json!({
            "database": ".yak/yak.db",
            "query": "insert CreateUser { user { name = \"Ada\" } }",
            "params": {}
        }),
    );
    assert_eq!(insert["ok"], true);
    assert_eq!(insert["results"][0]["operation"], "insert");

    let select = call_mcp_tool(
        &ctx,
        "pyre_query",
        json!({
            "database": ".yak/yak.db",
            "query": "query GetUsers { user { id name } }",
            "params": {}
        }),
    );

    assert_eq!(select["ok"], true);
    assert_eq!(select["results"][0]["operation"], "query");
    assert_eq!(select["results"][0]["response"]["user"][0]["name"], "Ada");
}

#[test]
fn pyre_query_reports_mutation_typecheck_failure() {
    let ctx = TestContext::new();
    write_schema(&ctx);

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    let response = call_mcp_tool_error(
        &ctx,
        "pyre_query",
        json!({
            "database": ".yak/yak.db",
            "query": "insert BadUser { user { name = 1 } }",
            "params": {}
        }),
    );

    assert_eq!(response["error"]["code"], -32603);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("typecheck"));
}

#[test]
fn pyre_query_reports_invalid_parameters() {
    let ctx = TestContext::new();
    write_schema(&ctx);

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    let response = call_mcp_tool_error(
        &ctx,
        "pyre_query",
        json!({
            "database": ".yak/yak.db",
            "query": "query UserByName($name: String) {\n    user {\n        @where { name == $name }\n        id\n        name\n    }\n}",
            "params": { "name": 42 }
        }),
    );

    assert_eq!(response["error"]["code"], -32603);
    let message = response["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("invalid input") || message.contains("expected"),
        "unexpected error message: {}",
        message
    );
}
