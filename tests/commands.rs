use std::path::PathBuf;
use tempfile::TempDir;
use assert_cmd::Command; // For testing CLI applications
use predicates::prelude::*; // For assertions on command output

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
    
    // Create a sample schema file
    std::fs::write(
        ctx.workspace_path.join("pyre/schema.sql"),
        "CREATE TABLE users (id INTEGER PRIMARY KEY);"
    ).unwrap();
    
    ctx.run_command("generate")
        .assert()
        .success()
        .stdout(predicate::str::contains("Generated"));
        
    // Verify generated files
    assert!(ctx.workspace_path.join("pyre/generated").exists());
}

#[test]
fn test_format_command() {
    let ctx = TestContext::new();
    
    // Create an unformatted query file
    std::fs::write(
        ctx.workspace_path.join("pyre/queries.sql"),
        "SELECT   *    FROM    users   WHERE   id   =   1;"
    ).unwrap();
    
    ctx.run_command("format")
        .assert()
        .success();
        
    // Verify formatted content
    let formatted = std::fs::read_to_string(
        ctx.workspace_path.join("pyre/queries.sql")
    ).unwrap();
    assert_eq!(formatted, "SELECT * FROM users WHERE id = 1;");
}