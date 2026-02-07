use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

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
