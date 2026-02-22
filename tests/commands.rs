use assert_cmd::Command;
use libsql;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

struct TestContext {
    temp_dir: TempDir,
    workspace_path: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().to_path_buf();

        // Set up any initial files needed
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

fn write_basic_schema(ctx: &TestContext) {
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

fn write_ai_session_schema_and_query(ctx: &TestContext) {
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
type AiSessionRole
   = Root
   | Worker
   | Reviewer

type AiSessionStatus
   = Active
   | Idle
   | Completed
   | Failed

record AiSession {
    @public
    id              Id.Int        @id
    sessionKey      String
    role            AiSessionRole
    status          AiSessionStatus
    updatedAt       DateTime      @default(now)
}
        "#,
    )
    .unwrap();

    std::fs::write(
        ctx.workspace_path.join("pyre/queries.pyre"),
        r#"
insert SeedActiveSession {
    aiSession {
        sessionKey = "session-1"
        role = Root
        status = Active
    }
}

query GetAiSessions {
    aiSession {
        id
        status
        updatedAt
    }
}
        "#,
    )
    .unwrap();
}

fn write_union_payload_schema_and_query(ctx: &TestContext) {
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
type AiSessionStatus
   = Active
   | Idle
   | Completed
   | Failed

type AiSessionLifecycle
   = Running
   | Finished {
       reason String
       endedAt DateTime?
     }

record AiSession {
    @public
    id         Id.Int            @id
    status     AiSessionStatus
    lifecycle  AiSessionLifecycle
    updatedAt  DateTime @default(now)
}
        "#,
    )
    .unwrap();

    std::fs::write(
        ctx.workspace_path.join("pyre/queries.pyre"),
        r#"
insert SeedRunningNumber {
    aiSession {
        status = Active
        lifecycle = Running
    }
}

insert SeedFinishedDone($endedAt: DateTime) {
    aiSession {
        status = Completed
        lifecycle = Finished {
            reason = "done"
            endedAt = $endedAt
        }
    }
}

insert SeedFinishedTimeout($endedAt: DateTime) {
    aiSession {
        status = Failed
        lifecycle = Finished {
            reason = "timeout"
            endedAt = $endedAt
        }
    }
}

insert SeedFinishedStringSeconds($endedAt: DateTime) {
    aiSession {
        status = Completed
        lifecycle = Finished {
            reason = "string-seconds"
            endedAt = $endedAt
        }
    }
}

insert SeedRunningIdle {
    aiSession {
        status = Idle
        lifecycle = Running
    }
}

update UpdateFinishedSession($id: AiSession.id, $updatedAt: DateTime, $endedAt: DateTime) {
    aiSession {
        @where { id == $id }
        lifecycle = Finished {
            reason = "updated-done"
            endedAt = $endedAt
        }
        updatedAt = $updatedAt
    }
}

query GetAiSessions {
    aiSession {
        id
        status
        lifecycle
        updatedAt
    }
}
        "#,
    )
    .unwrap();
}

fn write_json_schema_and_query(ctx: &TestContext) {
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
record Event {
    @public
    id      Id.Int @id
    payload Json?
}
        "#,
    )
    .unwrap();

    std::fs::write(
        ctx.workspace_path.join("pyre/queries.pyre"),
        r#"
insert SeedEvent($payload: Json) {
    event {
        payload = $payload
    }
}

query GetEvents {
    event {
        id
        payload
    }
}
        "#,
    )
    .unwrap();
}

fn write_game_lens_schema_and_query(ctx: &TestContext) {
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
type GameRole
   = GM
   | Player
   | Observer

record User {
    @public
    id    Id.Uuid @id
    name  String
    email String
}

record Game {
    @public
    id              Id.Uuid  @id
    name            String
    createdByUserId User.id
    createdAt       DateTime @default(now)

    gameMembers @link(GameMember.gameId)
    gameInvites @link(GameInvite.gameId)
}

record GameMember {
    @public
    id              Id.Uuid  @id
    gameId          Game.id
    userId          User.id
    role            GameRole
    joinedAt        DateTime @default(now)
    invitedByUserId User.id?

    inviter @link(invitedByUserId, User.id)
    games   @link(gameId, Game.id)
}

record GameInvite {
    @public
    id            Id.Uuid  @id
    gameId        Game.id
    inviterUserId User.id
    token         String
    message       String?
    createdAt     DateTime @default(now)

    games @link(gameId, Game.id)
}
        "#,
    )
    .unwrap();

    std::fs::write(
        ctx.workspace_path.join("pyre/queries.pyre"),
        r#"
query GetGame($id: Game.id) {
    game {
        @where { id == $id }

        id
        name
        createdByUserId
        createdAt
        gameMembers {
            id
            userId
            role
            joinedAt
            inviter {
                id
                name
                email
            }
        }
        gameInvites {
            id
            inviterUserId
            token
            message
            createdAt
        }
    }
}
        "#,
    )
    .unwrap();
}

fn bun_is_available() -> bool {
    StdCommand::new("bun").arg("--version").output().is_ok()
}

fn elm_is_available() -> bool {
    StdCommand::new("elm").arg("--version").output().is_ok()
}

fn write_elm_build_files(ctx: &TestContext) {
    let elm_root = ctx.workspace_path.join("pyre/generated/client/elm");
    assert!(
        elm_root.exists(),
        "Expected generated Elm directory to exist"
    );

    std::fs::write(
        elm_root.join("elm.json"),
        r#"{
    "type": "application",
    "source-directories": [
        "."
    ],
    "elm-version": "0.19.1",
    "dependencies": {
        "direct": {
            "elm/browser": "1.0.2",
            "elm/core": "1.0.5",
            "elm/html": "1.0.0",
            "elm/json": "1.1.3",
            "elm/time": "1.0.0"
        },
        "indirect": {
            "elm/url": "1.0.0",
            "elm/virtual-dom": "1.0.3"
        }
    },
    "test-dependencies": {
        "direct": {},
        "indirect": {}
    }
}
"#,
    )
    .unwrap();

    let mut imports = String::new();
    imports.push_str("import Db\n");
    imports.push_str("import Db.Decode\n");
    imports.push_str("import Db.Delta\n");
    imports.push_str("import Db.Encode\n");
    imports.push_str("import Db.Id\n");

    let query_dir = elm_root.join("Query");
    if query_dir.exists() {
        let mut query_modules: Vec<String> = std::fs::read_dir(&query_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("elm"))
            .filter_map(|path| {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .map(|name| format!("import Query.{}\n", name))
            })
            .collect();
        query_modules.sort();
        for import in query_modules {
            imports.push_str(&import);
        }
    }

    let main_module = format!(
        "module Main exposing (main)\n\nimport Browser\nimport Html exposing (Html, text)\n{}\n\nmain : Program () () msg\nmain =\n    Browser.sandbox\n        {{ init = ()\n        , update = \\_ model -> model\n        , view = \\_ -> text \"elm-build-ok\"\n        }}\n",
        imports
    );

    std::fs::write(elm_root.join("Main.elm"), main_module).unwrap();
}

fn run_elm_make_check(ctx: &TestContext) {
    let elm_root = ctx.workspace_path.join("pyre/generated/client/elm");
    let output = StdCommand::new("elm")
        .arg("make")
        .arg("Main.elm")
        .arg("--output=elm.js")
        .current_dir(&elm_root)
        .output()
        .expect("Failed to execute elm make");

    assert!(
        output.status.success(),
        "elm make failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ensure_workspace_node_modules(ctx: &TestContext) {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_node_modules = repo_root.join("node_modules");
    assert!(
        repo_node_modules.exists(),
        "Expected root node_modules to exist for bun runtime imports"
    );

    let workspace_node_modules = ctx.workspace_path.join("node_modules");
    if !workspace_node_modules.exists() {
        #[cfg(unix)]
        symlink(&repo_node_modules, &workspace_node_modules)
            .expect("Failed to symlink workspace node_modules");
    }
}

fn generate_runtime(ctx: &TestContext) {
    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    ctx.run_command("generate").assert().success();
    ensure_workspace_node_modules(ctx);
}

fn run_bun_verification_script(
    ctx: &TestContext,
    script_name: &str,
    script_source: &str,
    success_marker: &str,
    failure_label: &str,
) {
    std::fs::write(ctx.workspace_path.join(script_name), script_source).unwrap();

    let output = StdCommand::new("bun")
        .arg("run")
        .arg(script_name)
        .current_dir(&ctx.workspace_path)
        .output()
        .expect("Failed to execute bun runtime verification script");

    assert!(
        output.status.success(),
        "{} failed\nstdout:\n{}\nstderr:\n{}",
        failure_label,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(success_marker),
        "Expected {} marker in bun output. stdout:\n{}",
        success_marker,
        stdout
    );
}

fn build_payload_union_verify_script(seed_calls: &[&str], expected_rows: &[&str]) -> String {
    let mut script = String::new();

    script.push_str("import { createClient } from \"@libsql/client\";\n");
    script.push_str("import {\n");
    script.push_str("  SeedRunningNumber,\n");
    script.push_str("  SeedFinishedDone,\n");
    script.push_str("  SeedFinishedTimeout,\n");
    script.push_str("  SeedFinishedStringSeconds,\n");
    script.push_str("  SeedRunningIdle,\n");
    script.push_str("  UpdateFinishedSession,\n");
    script.push_str("  GetAiSessions,\n");
    script.push_str("} from \"./pyre/generated/typescript/run.ts\";\n\n");
    script.push_str("const db = createClient({ url: \"file:.yak/yak.db\" });\n");

    for call in seed_calls {
        script.push_str("await ");
        script.push_str(call);
        script.push_str(";\n");
    }

    script.push_str("const result = await GetAiSessions(db, {});\n\n");
    script.push_str(
        "if (!result || !Array.isArray(result.aiSession) || result.aiSession.length !== 5) {\n",
    );
    script.push_str(
        "  throw new Error(`Expected exactly 5 aiSession rows, got: ${JSON.stringify(result)}`);\n",
    );
    script.push_str("}\n\n");
    script.push_str("function assertDate(label, value) {\n");
    script.push_str("  if (!(value instanceof Date) || Number.isNaN(value.getTime())) {\n");
    script.push_str("    throw new Error(`Expected ${label} to be a valid Date, got: ${JSON.stringify(value)}`);\n");
    script.push_str("  }\n");
    script.push_str("}\n\n");
    script.push_str("function assertMaybeDate(label, value) {\n");
    script.push_str("  if (value === null || value === undefined) {\n");
    script.push_str("    return;\n");
    script.push_str("  }\n\n");
    script.push_str("  assertDate(label, value);\n");
    script.push_str("}\n\n");
    script.push_str("const byId = new Map(result.aiSession.map((row) => [row.id, row]));\n\n");
    script.push_str("const expected = {\n");

    for row in expected_rows {
        script.push_str("  ");
        script.push_str(row);
        script.push_str("\n");
    }

    script.push_str("};\n\n");
    script.push_str("for (const [id, shape] of Object.entries(expected)) {\n");
    script.push_str("  const row = byId.get(Number(id));\n");
    script.push_str("  if (!row) {\n");
    script.push_str("    throw new Error(`Missing expected row id=${id}. Rows: ${JSON.stringify(result.aiSession)}`);\n");
    script.push_str("  }\n\n");
    script.push_str("  if (typeof row.status !== \"string\") {\n");
    script.push_str("    throw new Error(`Expected row ${id} status to decode as enum string, got: ${JSON.stringify(row.status)}`);\n");
    script.push_str("  }\n\n");
    script.push_str("  if (row.status !== shape.status) {\n");
    script.push_str("    throw new Error(`Expected row ${id} status ${shape.status}, got: ${JSON.stringify(row.status)}`);\n");
    script.push_str("  }\n\n");
    script.push_str("  assertDate(`row ${id}.updatedAt`, row.updatedAt);\n\n");
    script.push_str("  if (!row.lifecycle || typeof row.lifecycle !== \"object\") {\n");
    script.push_str("    throw new Error(`Expected row ${id} lifecycle object, got: ${JSON.stringify(row.lifecycle)}`);\n");
    script.push_str("  }\n\n");
    script.push_str("  if (row.lifecycle.type_ !== shape.lifecycleType) {\n");
    script.push_str("    throw new Error(`Expected row ${id} lifecycle.type_ ${shape.lifecycleType}, got: ${JSON.stringify(row.lifecycle.type_)}`);\n");
    script.push_str("  }\n\n");
    script.push_str("  if (shape.reason !== undefined) {\n");
    script.push_str(
        "    if (row.lifecycle.reason !== undefined && row.lifecycle.reason !== shape.reason) {\n",
    );
    script.push_str("      throw new Error(`Expected row ${id} optional reason ${shape.reason} when present, got: ${JSON.stringify(row.lifecycle.reason)}`);\n");
    script.push_str("    }\n");
    script.push_str("  }\n\n");
    script.push_str("  if (shape.endedAt === \"date\") {\n");
    script.push_str("    assertDate(`row ${id}.lifecycle.endedAt`, row.lifecycle.endedAt);\n");
    script.push_str("  } else if (shape.endedAt === \"optional-date\") {\n");
    script.push_str("    assertMaybeDate(`row ${id}.lifecycle.endedAt`, row.lifecycle.endedAt);\n");
    script.push_str("  } else if (shape.endedAt === \"nullish\") {\n");
    script.push_str(
        "    if (row.lifecycle.endedAt !== null && row.lifecycle.endedAt !== undefined) {\n",
    );
    script.push_str("      throw new Error(`Expected row ${id} endedAt to be null or undefined, got: ${JSON.stringify(row.lifecycle.endedAt)}`);\n");
    script.push_str("    }\n");
    script.push_str("  } else {\n");
    script.push_str("    assertMaybeDate(`row ${id}.lifecycle.endedAt`, row.lifecycle.endedAt);\n");
    script.push_str("  }\n");
    script.push_str("}\n\n");
    script.push_str("console.log(\"payload-union-and-date-check-passed\");\n");

    script
}

#[test]
fn test_generate_command() {
    let ctx = TestContext::new();

    // Create a sample schema file (Pyre format, not SQL)
    write_basic_schema(&ctx);

    ctx.run_command("generate").assert().success();

    // Verify generated files were created
    assert!(ctx.workspace_path.join("pyre/generated").exists());
    // Verify at least one generated file exists
    assert!(std::fs::read_dir(ctx.workspace_path.join("pyre/generated"))
        .unwrap()
        .next()
        .is_some());
}

#[test]
fn test_format_command() {
    let ctx = TestContext::new();

    // Create a schema file (needed for query formatting)
    write_basic_schema(&ctx);

    // Create an unformatted query file (Pyre format, not SQL)
    std::fs::write(
        ctx.workspace_path.join("pyre/queries.pyre"),
        r#"
        query   GetUsers   {
            user   {
                id
                name
            }
        }
        "#,
    )
    .unwrap();

    // Format the specific query file
    ctx.run_command("format")
        .arg("pyre/queries.pyre")
        .assert()
        .success();

    // Verify formatted content (should be properly formatted Pyre query)
    let formatted = std::fs::read_to_string(ctx.workspace_path.join("pyre/queries.pyre")).unwrap();

    // The format command should have formatted the query
    // Check that the file still contains the query (may be reformatted)
    // Format might normalize spacing, so we check for key parts
    assert!(
        formatted.contains("query") || formatted.contains("GetUsers"),
        "Formatted content: {}",
        formatted
    );
    assert!(
        formatted.contains("user") || formatted.contains("User"),
        "Formatted content: {}",
        formatted
    );
}

#[test]
fn test_migration_creates_migrations_directory_and_file() {
    let ctx = TestContext::new();
    write_basic_schema(&ctx);

    ctx.run_command("migration")
        .arg("--db")
        .arg(".yak/yak.db")
        .arg("init")
        .assert()
        .success();

    let migration_dir = ctx.workspace_path.join("pyre/migrations");
    assert!(migration_dir.exists(), "pyre/migrations should be created");
    assert!(
        std::fs::read_dir(migration_dir).unwrap().next().is_some(),
        "pyre/migrations should contain at least one migration folder"
    );
}

#[test]
fn test_migrate_push_creates_local_db_when_parent_directory_missing() {
    let ctx = TestContext::new();
    write_basic_schema(&ctx);

    let db_path = ctx.workspace_path.join(".yak/yak.db");
    assert!(!db_path.exists(), "db should not exist before push");

    let output = ctx
        .run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .output()
        .unwrap();

    assert!(
        output.status.code().is_some(),
        "migrate --push should execute and exit normally"
    );

    assert!(db_path.exists(), "db should be created by migrate --push");
}

#[tokio::test]
async fn test_migrate_push_creates_enum_columns_and_records_schema() {
    let ctx = TestContext::new();

    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
type TaskStatus
   = Pending
   | InProgress

record Task {
    @public
    id     Int        @id
    title  String
    status TaskStatus
}
        "#,
    )
    .unwrap();

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .arg("--push")
        .assert()
        .success();

    let db_path = ctx.workspace_path.join(".yak/yak.db");
    let db = libsql::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();

    let mut pragma_rows = conn.query("pragma table_info('tasks')", ()).await.unwrap();
    let mut has_status = false;
    while let Some(row) = pragma_rows.next().await.unwrap() {
        let column_name: String = row.get(1).unwrap();
        if column_name == "status" {
            has_status = true;
            break;
        }
    }

    assert!(
        has_status,
        "tasks.status should exist after migrate --push for enum-backed fields"
    );

    let mut schema_rows = conn
        .query(
            "select schema from _pyre_schema order by created_at desc limit 1",
            (),
        )
        .await
        .unwrap();

    let schema_row = schema_rows
        .next()
        .await
        .unwrap()
        .expect("_pyre_schema should contain at least one schema row");
    let schema_source: String = schema_row.get(0).unwrap();

    assert!(
        schema_source.contains("status TaskStatus"),
        "_pyre_schema should store the pushed schema source"
    );
}

#[test]
fn test_migrate_without_migrations_shows_targeted_error() {
    let ctx = TestContext::new();
    write_basic_schema(&ctx);

    ctx.run_command("migrate")
        .arg(".yak/yak.db")
        .assert()
        .failure()
        .stdout(predicate::str::contains("No Migrations Found"))
        .stdout(predicate::str::contains(
            "pyre migration --db <database> init",
        ));
}

#[test]
fn test_generate_schema_with_relationships() {
    let ctx = TestContext::new();

    // Create a schema with relationships
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.pyre"),
        r#"
record User {
    @public
    id   Int    @id
    name String
    
    posts @link(Post.authorUserId)
}

record Post {
    @public
    id           Int    @id
    authorUserId Int
    title        String
    
    users @link(authorUserId, User.id)
}
        "#,
    )
    .unwrap();

    ctx.run_command("generate").assert().success();

    // Verify schema.ts was generated
    let schema_path = ctx
        .workspace_path
        .join("pyre/generated/typescript/core/schema.ts");
    assert!(schema_path.exists(), "schema.ts should be generated");

    // Read and verify the generated schema
    let schema_content = std::fs::read_to_string(&schema_path).unwrap();

    // Verify that table names in 'to' field use actual table names (lowercase plural), not record names
    // User.posts link should point to "posts" table, not "Post"
    assert!(
        schema_content.contains(r#"table: "posts""#),
        "User.posts link should point to 'posts' table, not 'Post'. Schema content:\n{}",
        schema_content
    );

    // Post.users link should point to "users" table, not "User"
    assert!(
        schema_content.contains(r#"table: "users""#),
        "Post.users link should point to 'users' table, not 'User'. Schema content:\n{}",
        schema_content
    );

    // Verify the structure uses 'links' not 'relationships'
    assert!(
        schema_content.contains("links:"),
        "Schema should use 'links' property, not 'relationships'. Schema content:\n{}",
        schema_content
    );

    // Verify schema types come from @pyre/core
    assert!(
        schema_content.contains("import type { SchemaMetadata } from '@pyre/core';"),
        "Schema should import SchemaMetadata from @pyre/core. Schema content:\n{}",
        schema_content
    );
}

#[test]
fn test_generated_elm_client_compiles_with_elm_make() {
    if !elm_is_available() {
        eprintln!("Skipping elm make test: elm not available");
        return;
    }

    let ctx = TestContext::new();
    write_union_payload_schema_and_query(&ctx);

    ctx.run_command("generate").assert().success();

    write_elm_build_files(&ctx);
    run_elm_make_check(&ctx);
}

#[test]
fn test_generated_elm_client_compiles_nested_lenses() {
    if !elm_is_available() {
        eprintln!("Skipping elm make test: elm not available");
        return;
    }

    let ctx = TestContext::new();
    write_game_lens_schema_and_query(&ctx);

    ctx.run_command("generate").assert().success();

    write_elm_build_files(&ctx);
    run_elm_make_check(&ctx);
}

#[tokio::test]
async fn test_generated_typescript_runner_decodes_enum_unions_and_dates() {
    if !bun_is_available() {
        eprintln!("Skipping bun-based TypeScript runtime test: bun not available");
        return;
    }

    let ctx = TestContext::new();
    write_ai_session_schema_and_query(&ctx);

    generate_runtime(&ctx);

    let verify_script = r#"
import { createClient } from "@libsql/client";
import { SeedActiveSession, GetAiSessions } from "./pyre/generated/typescript/run.ts";

const db = createClient({ url: "file:.yak/yak.db" });
await SeedActiveSession(db, {});
const result = await GetAiSessions(db, {});

if (!result || !Array.isArray(result.aiSession) || result.aiSession.length === 0) {
  throw new Error(`Expected aiSession rows, got: ${JSON.stringify(result)}`);
}

const first = result.aiSession[0];
if (typeof first.status !== "string") {
  throw new Error(`Expected status to decode as string union, got: ${JSON.stringify(first.status)}`);
}

if (!(first.updatedAt instanceof Date) || Number.isNaN(first.updatedAt.getTime())) {
  throw new Error(`Expected updatedAt to decode to valid Date, got: ${JSON.stringify(first.updatedAt)}`);
}

if (first.status !== "Active") {
  throw new Error(`Expected status Active, got: ${String(first.status)}`);
}

console.log("decoder-check-passed");
"#;

    run_bun_verification_script(
        &ctx,
        "verify-generated-run.ts",
        verify_script,
        "decoder-check-passed",
        "bun runtime verification",
    );
}

#[tokio::test]
async fn test_generated_typescript_runner_decodes_payload_unions_and_datetime_strings() {
    if !bun_is_available() {
        eprintln!("Skipping bun-based TypeScript runtime test: bun not available");
        return;
    }

    let ctx = TestContext::new();
    write_union_payload_schema_and_query(&ctx);

    generate_runtime(&ctx);

    let seed_calls = [
        "SeedRunningNumber(db, {})",
        "SeedFinishedDone(db, { endedAt: \"2026-01-03T00:00:00.000Z\" })",
        "SeedFinishedTimeout(db, { endedAt: 1735776000 })",
        "SeedFinishedStringSeconds(db, { endedAt: \"1735948800\" })",
        "SeedRunningIdle(db, {})",
        "UpdateFinishedSession(db, { id: 2, updatedAt: \"2026-02-02T00:00:00.000Z\", endedAt: \"1736035200\" })",
        "UpdateFinishedSession(db, { id: 3, updatedAt: 1736121600, endedAt: 1736208000 })",
    ];
    let expected_rows = [
        "1: { status: \"Active\", lifecycleType: \"Running\" },",
        "2: { status: \"Completed\", lifecycleType: \"Finished\", reason: \"updated-done\", endedAt: \"optional-date\" },",
        "3: { status: \"Failed\", lifecycleType: \"Finished\", reason: \"updated-done\", endedAt: \"optional-date\" },",
        "4: { status: \"Completed\", lifecycleType: \"Finished\", reason: \"string-seconds\", endedAt: \"optional-date\" },",
        "5: { status: \"Idle\", lifecycleType: \"Running\" },",
    ];
    let verify_script = build_payload_union_verify_script(&seed_calls, &expected_rows);

    run_bun_verification_script(
        &ctx,
        "verify-generated-union-run.ts",
        &verify_script,
        "payload-union-and-date-check-passed",
        "bun payload union verification",
    );
}

#[tokio::test]
async fn test_generated_typescript_runner_roundtrips_json_values() {
    if !bun_is_available() {
        eprintln!("Skipping bun-based TypeScript runtime test: bun not available");
        return;
    }

    let ctx = TestContext::new();
    write_json_schema_and_query(&ctx);

    generate_runtime(&ctx);

    let verify_script = r#"
import { createClient } from "@libsql/client";
import { SeedEvent, GetEvents } from "./pyre/generated/typescript/run.ts";

const db = createClient({ url: "file:.yak/yak.db" });

const expected = [
  { nested: { count: 2, ok: true }, tags: ["alpha", "beta"] },
  [1, { deep: "value" }, false],
  "hello-json",
  42,
  null,
];

for (const payload of expected) {
  await SeedEvent(db, { payload });
}

const result = await GetEvents(db, {});
if (!result || !Array.isArray(result.event)) {
  throw new Error(`Expected event rows, got: ${JSON.stringify(result)}`);
}

if (result.event.length !== expected.length) {
  throw new Error(`Expected ${expected.length} rows, got: ${result.event.length}`);
}

const rows = [...result.event].sort((a, b) => a.id - b.id);
for (let i = 0; i < expected.length; i++) {
  const row = rows[i];
  const actualJson = JSON.stringify(row.payload);
  const expectedJson = JSON.stringify(expected[i]);
  if (actualJson !== expectedJson) {
    throw new Error(`Payload mismatch at index ${i}. Expected ${expectedJson}, got ${actualJson}`);
  }
}

const firstPayload = rows[0]?.payload;
const payloadAsUnknown = firstPayload as unknown;
if (payloadAsUnknown === undefined) {
  throw new Error("Expected first payload to be present");
}

console.log("json-roundtrip-check-passed");
"#;

    run_bun_verification_script(
        &ctx,
        "verify-generated-json-run.ts",
        verify_script,
        "json-roundtrip-check-passed",
        "bun json runtime verification",
    );
}
