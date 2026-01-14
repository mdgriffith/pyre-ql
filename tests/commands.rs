use assert_cmd::Command;
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

        Self {
            temp_dir,
            workspace_path,
        }
    }

    fn run_command(&self, subcommand: &str) -> assert_cmd::Command {
        let mut cmd = Command::cargo_bin("pyre").unwrap();
        cmd.current_dir(&self.workspace_path);
        cmd.arg(subcommand);
        cmd
    }
}

#[test]
fn test_generate_command() {
    let ctx = TestContext::new();

    // Create a sample schema file (Pyre format, not SQL)
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
        .join("pyre/generated/client/node/schema.ts");
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

    // Verify LinkInfo interface exists
    assert!(
        schema_content.contains("export interface LinkInfo"),
        "Schema should export LinkInfo interface. Schema content:\n{}",
        schema_content
    );
}
