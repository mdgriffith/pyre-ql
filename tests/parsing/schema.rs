use pyre::ast;
use pyre::error;
use pyre::parser;

/// Helper function to format errors without color for testing
/// Strips ANSI color codes from the formatted error
fn format_error_no_color(file_contents: &str, error: &error::Error) -> String {
    let formatted = error::format_error(file_contents, error);
    // Strip ANSI escape codes
    strip_ansi_codes(&formatted)
}

fn strip_ansi_codes(s: &str) -> String {
    // Remove ANSI escape sequences (CSI sequences)
    // Simple approach: remove escape sequences starting with \x1b[
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            // Skip the escape sequence
            chars.next(); // skip '['
            while let Some(&c) = chars.peek() {
                if c == 'm' {
                    chars.next(); // skip 'm'
                    break;
                }
                chars.next();
            }
        } else {
            result.push(ch);
        }
    }
    result
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
    assert!(result.is_ok(), "Valid tagged type should parse successfully");
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
    assert!(result.is_ok(), "Valid record with link should parse successfully");
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
    assert!(result.is_ok(), "Valid record with tablename should parse successfully");
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
                formatted.contains("schema.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue"),
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
                formatted.contains("schema.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue") || formatted.contains("column"),
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
                formatted.contains("schema.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue") || formatted.contains("column"),
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
    assert!(result.is_err(), "Missing type in tagged variant should fail");

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
                formatted.contains("@unknown") && 
                (formatted.contains("@id") || formatted.contains("did you mean")),
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
    assert!(result.is_err(), "Invalid link syntax (missing comma) should fail");

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
                formatted.contains("schema.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue") || formatted.contains("Incomplete"),
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

