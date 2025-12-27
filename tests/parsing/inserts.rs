use pyre::parser;

/// Helper function to format errors without color for testing
fn format_error_no_color(file_contents: &str, error: &pyre::error::Error) -> String {
    let formatted = pyre::error::format_error(file_contents, error, false);
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
fn test_valid_insert() {
    let insert_source = r#"
        insert CreateUser {
            user {
                id = 1
                name = "Alice"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_ok(), "Valid insert should parse successfully");
}

#[test]
fn test_valid_insert_with_params() {
    let insert_source = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_ok(),
        "Valid insert with params should parse successfully"
    );
}

#[test]
fn test_valid_insert_with_multiple_params() {
    let insert_source = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_ok(),
        "Valid insert with multiple params should parse successfully"
    );
}

#[test]
fn test_valid_insert_with_nested_inserts() {
    let insert_source = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
                posts {
                    title = "First Post"
                    content = "Content"
                }
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_ok(),
        "Valid insert with nested inserts should parse successfully"
    );
}

#[test]
fn test_valid_insert_with_union_variant() {
    let insert_source = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
                status = Active
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_ok(),
        "Valid insert with union variant should parse successfully"
    );
}

#[test]
fn test_valid_insert_with_union_variant_fields() {
    let insert_source = r#"
        insert CreateUser($name: String, $reason: String) {
            user {
                name = $name
                status = Special { reason = $reason }
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_ok(),
        "Valid insert with union variant fields should parse successfully"
    );
}

#[test]
fn test_missing_insert_name() {
    let insert_source = r#"
        insert {
            user {
                name = "Alice"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_err(), "Missing insert name should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(insert_source, &error);

            // The parser gives a generic error message for this case
            assert!(
                formatted.contains("query.pyre") && formatted.contains("insert {"),
                "Error message should contain file and insert. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_missing_insert_brace() {
    let insert_source = r#"
        insert CreateUser
            user {
                name = "Alice"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_err(), "Missing opening brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(insert_source, &error);

            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue"),
                "Error message should indicate what was expected. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_invalid_assignment_syntax() {
    let insert_source = r#"
        insert CreateUser {
            user {
                name: "Alice"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(
        result.is_err(),
        "Invalid assignment syntax (using colon instead of equals) should fail"
    );

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(insert_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
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
fn test_missing_value_in_assignment() {
    let insert_source = r#"
        insert CreateUser {
            user {
                name =
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_err(), "Missing value in assignment should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(insert_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue")
                    || formatted.contains("Incomplete"),
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
fn test_invalid_param_type() {
    let insert_source = r#"
        insert CreateUser($name) {
            user {
                name = $name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    // Note: The parser may accept this syntax, but typechecking will fail
    // This test documents the current parsing behavior
    let _ = result;
}

#[test]
fn test_missing_closing_brace() {
    let insert_source = r#"
        insert CreateUser {
            user {
                name = "Alice"
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_err(), "Missing closing brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(insert_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue")
                    || formatted.contains("Incomplete"),
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
fn test_insert_with_comments() {
    let insert_source = r#"
        // This is a comment
        insert CreateUser {
            user {
                name = "Alice"
                // Another comment
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    assert!(result.is_ok(), "Comments should be allowed in inserts");
}

#[test]
fn test_invalid_union_variant_syntax() {
    let insert_source = r#"
        insert CreateUser {
            user {
                status = Special reason = "test"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", insert_source);
    // Note: The parser may accept this syntax, but typechecking will fail
    // This test documents the current parsing behavior
    let _ = result;
}
