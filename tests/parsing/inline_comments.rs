use pyre::ast;
use pyre::parser;

#[test]
fn test_inline_comment_after_directive() {
    let schema_source = r#"
record Task {
    id String @id // This is an inline comment
    name String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comment after directive should be allowed. Error: {:?}",
        result.err()
    );

    // Verify the record was parsed correctly
    assert_eq!(schema.files.len(), 1);
    let file = &schema.files[0];
    let record_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Record { .. }))
        .count();
    assert_eq!(record_count, 1);
}

#[test]
fn test_inline_comment_after_field_type() {
    let schema_source = r#"
record Task {
    id String // This is an inline comment after the type
    name String @unique // And one after a directive
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comment after field type should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_after_multiple_directives() {
    let schema_source = r#"
record Task {
    id String @id @unique // Comment after multiple directives
    email String @unique @index // Another inline comment
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comment after multiple directives should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_with_special_characters() {
    let schema_source = r#"
record Task {
    id String @id // Comment with @special #characters!
    name String // Comment with $variables and (parentheses)
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comments with special characters should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_empty() {
    let schema_source = r#"
record Task {
    id String @id //
    name String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Empty inline comment should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_mixed_inline_and_standalone_comments() {
    let schema_source = r#"
// Standalone comment
record Task {
    id String @id // Inline comment
    // Another standalone comment
    name String
    status String // Another inline comment
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Mixed inline and standalone comments should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_in_session() {
    let schema_source = r#"
session {
    userId Int // User identifier
    role String // User role
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comments in session should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_in_tagged_type() {
    let schema_source = r#"
type Status
   = Active // Currently active
   | Inactive // Not active anymore
   | Pending { // Waiting for approval
        reason String // Reason for pending
     }
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comments in tagged type should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_after_link_directive() {
    let schema_source = r#"
record User {
    id Int @id
}

record Post {
    id Int @id
    authorId Int
    author @link(authorId, User.id) // Link to user
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comment after link directive should be allowed. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_inline_comment_after_default_directive() {
    let schema_source = r#"
record Task {
    id String @id
    createdAt DateTime @default(now) // Set on creation
    status String @default("pending") // Default status
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Inline comment after default directive should be allowed. Error: {:?}",
        result.err()
    );
}
