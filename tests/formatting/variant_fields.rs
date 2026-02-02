use pyre::ast;
use pyre::format;
use pyre::generate;
use pyre::parser;

/// Helper to format and return string
fn format_schema(source: &str) -> String {
    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", source, &mut schema);
    assert!(
        result.is_ok(),
        "Parse should succeed. Error: {:?}",
        result.err()
    );
    format::schema(&mut schema);
    generate::to_string::schema_to_string(&schema.namespace, &schema)
}

#[test]
fn test_variant_single_line_stays_single_line() {
    let schema_source = r#"
type DevActionType
    = Navigate { to String }
    | Click { selector String }
    | Type { selector String, text String, clear Bool? }
"#;

    let formatted = format_schema(schema_source);

    // Format again to ensure idempotence
    let formatted_again = format_schema(&formatted);

    // Should remain single line
    assert!(
        formatted.contains("Navigate { to String }"),
        "Navigate should stay on single line. Got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("Click { selector String }"),
        "Click should stay on single line. Got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("Type { selector String, text String, clear Bool? }"),
        "Type should stay on single line. Got:\n{}",
        formatted
    );

    // Should be idempotent
    assert_eq!(
        formatted, formatted_again,
        "Formatting should be idempotent. First:\n{}\n\nSecond:\n{}",
        formatted, formatted_again
    );
}

#[test]
fn test_variant_multiline_stays_multiline() {
    let schema_source = r#"
type DevEventType
    = Log {
        level DevLogLevel
        message String
        payload Json?
    }
    | Navigation {
        to String
        from String?
    }
"#;

    let formatted = format_schema(schema_source);

    // Format again to ensure idempotence
    let formatted_again = format_schema(&formatted);

    // Should remain multiline
    assert!(
        formatted.contains("Log {\n"),
        "Log should be multiline. Got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("Navigation {\n"),
        "Navigation should be multiline. Got:\n{}",
        formatted
    );

    // Should be idempotent
    assert_eq!(
        formatted, formatted_again,
        "Formatting should be idempotent. First:\n{}\n\nSecond:\n{}",
        formatted, formatted_again
    );
}

#[test]
fn test_variant_mixed_formats() {
    let schema_source = r#"
type ActionType
    = Simple
    | Navigate { to String }
    | Complex {
        selector String
        modifier String?
    }
"#;

    let formatted = format_schema(schema_source);
    let formatted_again = format_schema(&formatted);

    // Simple should stay simple (no fields)
    assert!(
        formatted.contains("Simple\n"),
        "Simple should have no fields. Got:\n{}",
        formatted
    );

    // Navigate should stay single line
    assert!(
        formatted.contains("Navigate { to String }"),
        "Navigate should stay on single line. Got:\n{}",
        formatted
    );

    // Complex should stay multiline
    assert!(
        formatted.contains("Complex {\n"),
        "Complex should be multiline. Got:\n{}",
        formatted
    );

    // Should be idempotent
    assert_eq!(
        formatted, formatted_again,
        "Formatting should be idempotent. First:\n{}\n\nSecond:\n{}",
        formatted, formatted_again
    );
}

#[test]
fn test_variant_long_line_breaks_to_multiline() {
    // This line is longer than 80 characters
    let schema_source = r#"
type LongType
    = VeryLongVariantName { veryLongFieldName String, anotherLongFieldName String, yetAnotherLongFieldName Int }
"#;

    let formatted = format_schema(schema_source);

    // Should break to multiline since it exceeds 80 chars
    assert!(
        formatted.contains("VeryLongVariantName {\n"),
        "Long variant should break to multiline. Got:\n{}",
        formatted
    );
}

#[test]
fn test_variant_explicit_newline_after_comma() {
    // User adds a newline after comma - should format as multiline
    let schema_source = r#"
type DevActionType
    = Type { selector String,
        text String, clear Bool? }
"#;

    let formatted = format_schema(schema_source);

    // Should be formatted as multiline because user added explicit newline
    assert!(
        formatted.contains("Type {\n"),
        "Should format as multiline when user adds newline. Got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("        selector"),
        "Fields should be indented. Got:\n{}",
        formatted
    );
}

#[test]
fn test_variant_exactly_80_chars() {
    // Test the boundary - exactly 80 characters should stay single line
    // "   = MyVariant { " = 17 chars, closing " }" = 2 chars, leaves 61 for fields
    let schema_source = r#"
type BoundaryTest
    = Var { fieldOne String, fieldTwo Int, fieldThree Bool }
"#;

    let formatted = format_schema(schema_source);
    let line_with_var = formatted
        .lines()
        .find(|l| l.contains("Var {"))
        .expect("Should find Var line");

    // If it's under 80, should stay single line
    if line_with_var.len() <= 80 {
        assert!(
            line_with_var.contains("}"),
            "Should stay on single line. Got:\n{}",
            formatted
        );
    }
}

#[test]
fn test_variant_formatting_idempotence() {
    let test_cases = vec![
        // Single line variants
        r#"
type T1
    = A { x Int }
    | B { x Int, y String }
"#,
        // Multiline variants
        r#"
type T2
    = A {
        x Int
        y String
    }
"#,
        // Mixed
        r#"
type T3
    = A
    | B { x Int }
    | C {
        x Int
        y String
    }
"#,
    ];

    for (i, source) in test_cases.iter().enumerate() {
        let formatted_once = format_schema(source);
        let formatted_twice = format_schema(&formatted_once);
        let formatted_thrice = format_schema(&formatted_twice);

        assert_eq!(
            formatted_once, formatted_twice,
            "Test case {} failed: First and second format differ.\nFirst:\n{}\n\nSecond:\n{}",
            i, formatted_once, formatted_twice
        );

        assert_eq!(
            formatted_twice, formatted_thrice,
            "Test case {} failed: Second and third format differ.\nSecond:\n{}\n\nThird:\n{}",
            i, formatted_twice, formatted_thrice
        );
    }
}

#[test]
fn test_variant_with_nullable_fields() {
    let schema_source = r#"
type Result
    = Success { value String? }
    | Error { code Int, message String? }
"#;

    let formatted = format_schema(schema_source);
    let formatted_again = format_schema(&formatted);

    // Should stay single line
    assert!(
        formatted.contains("Success { value String? }"),
        "Success with nullable should stay single line. Got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("Error { code Int, message String? }"),
        "Error with nullable should stay single line. Got:\n{}",
        formatted
    );

    // Should be idempotent
    assert_eq!(formatted, formatted_again);
}

#[test]
fn test_variant_all_on_same_line() {
    // All variants on same line as type declaration (edge case)
    let schema_source = r#"
type Compact = A { x Int } | B { y String }
"#;

    let result = format_schema(schema_source);
    // Should parse and format correctly
    assert!(result.contains("type Compact"));
}
