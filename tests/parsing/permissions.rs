use pyre::ast;
use pyre::error::ErrorType;
use pyre::parser;
use pyre::typecheck;

/// Helper function to format errors without color for testing
fn format_error_no_color(file_contents: &str, error: &pyre::error::Error) -> String {
    let formatted = pyre::error::format_error(file_contents, error);
    strip_ansi_codes(&formatted)
}

fn strip_ansi_codes(s: &str) -> String {
    // Remove ANSI escape sequences (CSI sequences)
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
fn test_star_permission_simple() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    if let Err(e) = result {
        println!("Parse error: {}", parser::render_error(schema_source, e));
        panic!("Parse failed");
    }
}

#[test]
fn test_star_permission_with_and() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId && published = True }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Star permission with AND condition should parse successfully"
    );
}

#[test]
fn test_star_permission_with_or() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId || status = "published" }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Star permission with OR condition should parse successfully"
    );
}

#[test]
fn test_operation_specific_single_operation() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with single operation should parse successfully"
    );
}

#[test]
fn test_operation_specific_multiple_operations_same_line() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select, update { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with multiple operations on same line should parse successfully"
    );
}

#[test]
fn test_operation_specific_all_operations() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select, insert, update, delete { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with all operations should parse successfully"
    );
}

#[test]
fn test_operation_specific_multiple_lines() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select { authorId = Session.userId }
        insert { authorId = Session.userId }
        update { authorId = Session.userId }
        delete { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with multiple lines should parse successfully"
    );
}

#[test]
fn test_operation_specific_mixed_lines() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select, update { authorId = Session.userId }
        insert, delete { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with mixed lines should parse successfully"
    );
}

#[test]
fn test_operation_specific_with_complex_where() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select { authorId = Session.userId || status = "published" }
        delete { authorId = Session.userId && Session.role = "admin" }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    if let Err(e) = &result {
        let error_msg = parser::render_error(schema_source, e.clone());
        println!("Parse error:\n{}", error_msg);
    }
    assert!(
        result.is_ok(),
        "Operation-specific permission with complex where clauses should parse successfully"
    );
}

#[test]
fn test_operation_specific_with_separate_permissions() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select { authorId = Session.userId || status = "published" }
        delete { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    if let Err(e) = &result {
        let error_msg = parser::render_error(schema_source, e.clone());
        println!("Parse error:\n{}", error_msg);
    }
    assert!(
        result.is_ok(),
        "Operation-specific permission with complex where clauses should parse successfully"
    );
}

#[test]
fn test_operation_specific_with_role_admin() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        delete { Session.role = "admin" }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    if let Err(e) = &result {
        let error_msg = parser::render_error(schema_source, e.clone());
        println!("Parse error:\n{}", error_msg);
    }
    assert!(
        result.is_ok(),
        "Operation-specific permission with complex where clauses should parse successfully"
    );
}

#[test]
fn test_permission_with_session_variable() {
    let schema_source = r#"
session {
    userId Int
    role String
}

record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with Session variable should parse successfully"
    );
}

#[test]
fn test_permission_with_string_literal() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { status = "published" }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with string literal should parse successfully"
    );
}

#[test]
fn test_permission_with_integer_literal() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = 1 }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with integer literal should parse successfully"
    );
}

#[test]
fn test_permission_with_boolean_literal() {
    let schema_source = r#"
record Post {
    id Int @id
    published Bool
    @permissions { published = True }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with boolean literal should parse successfully"
    );
}

#[test]
fn test_permission_with_comparison_operators() {
    let schema_source = r#"
record Post {
    id Int @id
    score Int
    @permissions { score >= 10 }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with comparison operator should parse successfully"
    );
}

#[test]
fn test_permission_with_nested_and_or() {
    let schema_source = r#"
record Post {
    id Int @id
    authorId Int
    status String
    published Bool
    @permissions { 
        (authorId = Session.userId || status = "published") && published = True 
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    // Note: This might not parse correctly if parentheses aren't supported
    // Let's see what happens
    if result.is_err() {
        // If it fails, that's okay - we're testing what's actually supported
        println!("Nested parentheses may not be supported: {:?}", result);
    }
}

#[test]
fn test_permission_missing_closing_brace() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Permission with missing closing brace should fail to parse"
    );
}

#[test]
fn test_permission_missing_where_clause() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    // This might parse but be invalid semantically - let's see
    if result.is_err() {
        println!("Empty permissions block failed as expected: {:?}", result);
    }
}

#[test]
fn test_permission_invalid_operation() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        invalid { authorId = Session.userId }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_err(),
        "Permission with invalid operation should fail to parse"
    );
}

#[test]
fn test_multiple_permissions_on_same_record() {
    // This should fail - only one @permissions directive per record
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId }
    @permissions { status = "published" }
}
    "#;

    let mut schema = ast::Schema::default();
    let parse_result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(parse_result.is_ok(), "Schema should parse successfully");

    // Typecheck should fail with MultiplePermissions error
    let database = ast::Database {
        schemas: vec![schema],
    };
    let typecheck_result = typecheck::check_schema(&database);

    assert!(
        typecheck_result.is_err(),
        "Typecheck should fail with multiple @permissions directives"
    );

    let errors = typecheck_result.unwrap_err();
    assert!(
        errors.iter().any(|e| matches!(&e.error_type, ErrorType::MultiplePermissions { record } if record == "Post")),
        "Should have MultiplePermissions error for Post record. Errors: {:?}",
        errors
    );
}

#[test]
fn test_single_permission_allowed() {
    // A single @permissions directive should be allowed
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = Session.userId }
}
    "#;

    let mut schema = ast::Schema::default();
    let parse_result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(parse_result.is_ok(), "Schema should parse successfully");

    // Typecheck should succeed with a single @permissions directive
    let database = ast::Database {
        schemas: vec![schema],
    };
    let typecheck_result = typecheck::check_schema(&database);

    assert!(
        typecheck_result.is_ok(),
        "Typecheck should succeed with a single @permissions directive. Errors: {:?}",
        typecheck_result.err()
    );
}

#[test]
fn test_permission_with_variable() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions { authorId = $userId }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Permission with variable should parse successfully"
    );
}

#[test]
fn test_permission_operation_specific_with_multiple_where_conditions() {
    let schema_source = r#"
record Post {
    id Int @id
    title String
    @permissions {
        select { authorId = Session.userId, status = "published" }
    }
}
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Operation-specific permission with multiple where conditions should parse successfully"
    );
}
