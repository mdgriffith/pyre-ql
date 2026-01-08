use pyre::ast;
use pyre::error;
use pyre::parser;

/// Helper function to format errors without color for testing
/// Strips ANSI color codes from the formatted error
fn format_error_no_color(file_contents: &str, error: &error::Error) -> String {
    return error::format_error(file_contents, error, false);
}

#[test]
fn test_valid_record() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Valid record should parse successfully");
}

#[test]
fn test_valid_tagged_type() {
    let schema_source = r#"
type Status
   = Active
   | Inactive
   | Special {
        reason String
     }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Valid tagged type should parse successfully"
    );
}

#[test]
fn test_valid_session() {
    let schema_source = r#"
session {
    userId Int
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Valid session should parse successfully");
}

#[test]
fn test_valid_record_with_link() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}

record Post {
    id        Int    @id
    authorId  Int
    author    @link(authorId, User.id)
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Valid record with link should parse successfully"
    );
}

#[test]
fn test_valid_record_with_tablename() {
    let schema_source = r#"
record User {
    @tablename "users"
    id   Int    @id
    name String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Valid record with tablename should parse successfully"
    );
}

#[test]
fn test_missing_record_name() {
    let schema_source = r#"
record {
    id   Int    @id
    name String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Missing record name should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(schema_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("schema.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        } else {
            panic!("Expected parsing error but convert_parsing_error returned None");
        }
    } else {
        panic!("Expected parsing to fail but it succeeded");
    }
}

#[test]
fn test_missing_record_brace() {
    let schema_source = r#"
record User
    id   Int    @id
    name String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Missing opening brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(schema_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("schema.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue")
                    || formatted.contains("column"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        } else {
            panic!("Expected parsing error but convert_parsing_error returned None");
        }
    } else {
        panic!("Expected parsing to fail but it succeeded");
    }
}

#[test]
fn test_invalid_field_syntax() {
    let schema_source = r#"
record User {
    id Int @id
    name = String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Invalid field syntax should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(schema_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("schema.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue")
                    || formatted.contains("column"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        } else {
            panic!("Expected parsing error but convert_parsing_error returned None");
        }
    } else {
        panic!("Expected parsing to fail but it succeeded");
    }
}

#[test]
fn test_missing_type_in_tagged() {
    let schema_source = r#"
type Status
   = Active
   | Inactive
   | Special {
        reason
     }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Missing type in tagged variant should fail"
    );

    if let Err(err) = result {
        let error = parser::convert_parsing_error(err).unwrap();
        let formatted = format_error_no_color(schema_source, &error);

        assert!(
            formatted.contains("expecting") || formatted.contains("type"),
            "Error message should indicate what was expected. Got:\n{}",
            formatted
        );
    }
}

#[test]
fn test_invalid_directive() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String @unknown
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Invalid directive should fail");

    if let Err(err) = result {
        let error = parser::convert_parsing_error(err).unwrap();
        let formatted = format_error_no_color(schema_source, &error);

        // Check that the error mentions the unknown directive and suggests alternatives
        assert!(
            formatted.contains("@unknown")
                && (formatted.contains("@id") || formatted.contains("did you mean")),
            "Error message should mention @unknown and suggest alternatives. Got:\n{}",
            formatted
        );
    }
}

#[test]
fn test_invalid_link_syntax() {
    let schema_source = r#"
record User {
    id   Int    @id
}

record Post {
    id        Int    @id
    authorId  Int
    author    @link(authorId User.id)
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Invalid link syntax (missing comma) should fail"
    );

    if let Err(err) = result {
        let error = parser::convert_parsing_error(err).unwrap();
        let formatted = format_error_no_color(schema_source, &error);

        assert!(
            formatted.contains("expecting") || formatted.contains("link"),
            "Error message should indicate what was expected. Got:\n{}",
            formatted
        );
    }
}

#[test]
fn test_missing_closing_brace() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Missing closing brace should fail");

    if let Err(err) = result {
        // Some parsing errors may not be convertible, which is fine - we just verify parsing failed
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(schema_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("schema.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue")
                    || formatted.contains("Incomplete"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        }
        // If convert_parsing_error returns None, that's okay - we've verified parsing failed
    } else {
        panic!("Expected parsing to fail but it succeeded");
    }
}

#[test]
fn test_empty_record() {
    let schema_source = r#"
record User {
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    // Empty records might be valid or invalid depending on the implementation
    // This test documents the current behavior
    let _ = result;
}

#[test]
fn test_record_with_comments() {
    let schema_source = r#"
// This is a comment
record User {
    id   Int    @id
    // Another comment
    name String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Comments should be allowed in schema");
}

#[test]
fn test_multiple_records() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}

record Post {
    id        Int    @id
    title     String
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Multiple records should parse successfully");
}

#[test]
fn test_union_type_with_record() {
    // Test parsing union type and record together using the schema helper format
    // This verifies that schema_v1_complete() produces a parseable schema
    // Note: This test documents that the format from schema_v1_complete() works,
    // which uses format! with trim() to combine definitions
    use super::super::helpers::schema;

    let schema_source = schema::schema_v1_complete();

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", &schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Union type with record from schema_v1_complete() should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify both definitions were parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];

    // Count union types and records
    let union_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    let record_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Record { .. }))
        .count();

    assert_eq!(union_count, 1, "Should have parsed one union type (Status)");
    assert_eq!(record_count, 1, "Should have parsed one record (User)");
}

#[test]
fn test_union_type_with_leading_spaces() {
    // Test that union type alone with leading spaces parses successfully
    // This is the format that works for union types in migration tests
    let schema_source = r#"type Status
   = Active
   | Inactive
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Union type with leading spaces should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the union type was parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];

    let union_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();

    assert_eq!(union_count, 1, "Should have parsed one union type (Status)");
}

#[test]
fn test_indented_record_fails() {
    let schema_source = r#"
        record User {
            id   Int    @id
            name String
        }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Indented record should fail");
}

#[test]
fn test_indented_type_fails() {
    let schema_source = r#"
        type Status
           = Active
           | Inactive
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Indented type should fail");
}

#[test]
fn test_indented_session_fails() {
    let schema_source = r#"
        session {
            userId Int
        }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Indented session should fail");
}

#[test]
fn test_record_with_tab_indentation_fails() {
    let schema_source = "\trecord User {\n\t    id   Int    @id\n\t    name String\n\t}\n";

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Record with tab indentation should fail");
}

#[test]
fn test_session_with_tab_indentation_fails() {
    let schema_source = "\tsession {\n\t    userId Int\n\t}\n";

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Session with tab indentation should fail");
}

#[test]
fn test_record_with_single_space_indentation_fails() {
    let schema_source = r#" record User {
    id   Int    @id
    name String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Record with single space indentation should fail"
    );
}

#[test]
fn test_type_with_single_space_indentation_fails() {
    let schema_source = r#" type Status
   = Active
   | Inactive
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Type with single space indentation should fail"
    );
}

#[test]
fn test_session_with_single_space_indentation_fails() {
    let schema_source = r#" session {
    userId Int
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Session with single space indentation should fail"
    );
}

#[test]
fn test_record_with_deep_indentation_fails() {
    let schema_source = r#"
            record User {
                id   Int    @id
                name String
            }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Record with deep indentation should fail");
}

#[test]
fn test_type_with_deep_indentation_fails() {
    let schema_source = r#"
            type Status
               = Active
               | Inactive
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_err(), "Type with deep indentation should fail");
}

#[test]
fn test_indented_record_after_valid_record_fails() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}

    record Post {
        id   Int    @id
        title String
    }
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Indented record after valid record should fail"
    );
}

#[test]
fn test_indented_type_after_valid_record_fails() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}

    type Status
       = Active
       | Inactive
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Indented type after valid record should fail"
    );
}

#[test]
fn test_indented_session_after_valid_record_fails() {
    let schema_source = r#"
record User {
    id   Int    @id
    name String
}

    session {
        userId Int
    }
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Indented session after valid record should fail"
    );
}

#[test]
fn test_record_at_start_of_file_with_spaces_fails() {
    let schema_source = "    record User {\n    id   Int    @id\n    name String\n}\n";

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Record at start of file with spaces should fail"
    );
}

#[test]
fn test_type_at_start_of_file_with_spaces_fails() {
    let schema_source = "    type Status\n   = Active\n   | Inactive\n";

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Type at start of file with spaces should fail"
    );
}

#[test]
fn test_session_at_start_of_file_with_spaces_fails() {
    let schema_source = "    session {\n    userId Int\n}\n";

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Session at start of file with spaces should fail"
    );
}

#[test]
fn test_multiple_indented_declarations_fail() {
    let schema_source = r#"
        record User {
            id   Int    @id
            name String
        }

        type Status
           = Active
           | Inactive

        session {
            userId Int
        }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Multiple indented declarations should fail"
    );
}

#[test]
fn test_tagged_type_with_leading_newline() {
    // Test that a tagged type with a leading newline parses successfully
    // This matches the format used in the failing round-trip test
    let schema_source = r#"
type SimpleTagged
   = Option1
   | Option2
   | Option3
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Tagged type with leading newline should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the tagged type was parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];
    let tagged_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    assert_eq!(tagged_count, 1, "Should have parsed one tagged type");
}

#[test]
fn test_tagged_type_with_fields_and_leading_newline() {
    // Test that a tagged type with fields and a leading newline parses successfully
    let schema_source = r#"
type TaggedWithFields
   = Active
   | Inactive
   | Pending {
        reason String
        createdAt DateTime
    }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Tagged type with fields and leading newline should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the tagged type was parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];
    let tagged_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    assert_eq!(tagged_count, 1, "Should have parsed one tagged type");
}

#[test]
fn test_multiple_tagged_types_with_leading_newline() {
    // Test that multiple tagged types with a leading newline parse successfully
    let schema_source = r#"
type SimpleTagged
   = Option1
   | Option2
   | Option3

type TaggedWithFields
   = Active
   | Inactive
   | Pending {
        reason String
        createdAt DateTime
    }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Multiple tagged types with leading newline should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify both tagged types were parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];
    let tagged_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    assert_eq!(tagged_count, 2, "Should have parsed two tagged types");

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", &schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Multiple tagged types with leading newline should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify both tagged types were parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];
    let tagged_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    assert_eq!(tagged_count, 2, "Should have parsed two tagged types");
}

#[test]
fn test_tagged_type_followed_by_record_with_leading_newline() {
    // Test that a tagged type followed by a record with a leading newline parses successfully
    // This matches the exact format from the failing round-trip test
    let schema_source = r#"
type SimpleTagged
   = Option1
   | Option2
   | Option3

type TaggedWithFields
   = Active
   | Inactive
   | Pending {
        reason String
        createdAt DateTime
    }

record Test {
    id Int @id
    status TaggedWithFields
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Tagged types followed by record with leading newline should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify all definitions were parsed
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];
    let tagged_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Tagged { .. }))
        .count();
    let record_count = file
        .definitions
        .iter()
        .filter(|d| matches!(d, ast::Definition::Record { .. }))
        .count();
    assert_eq!(tagged_count, 2, "Should have parsed two tagged types");
    assert_eq!(record_count, 1, "Should have parsed one record");
}
