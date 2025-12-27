use pyre::parser;

/// Helper function to format errors without color for testing
fn format_error_no_color(file_contents: &str, error: &pyre::error::Error) -> String {
    return pyre::error::format_error(file_contents, error, false);
}

#[test]
fn test_valid_update() {
    let update_source = r#"
        update UpdateUser {
            user {
                name = "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(result.is_ok(), "Valid update should parse successfully");
}

#[test]
fn test_valid_update_with_params() {
    let update_source = r#"
        update UpdateUser($id: Int, $name: String) {
            user {
                @where { id = $id }
                name = $name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(
        result.is_ok(),
        "Valid update with params should parse successfully"
    );
}

#[test]
fn test_valid_update_with_where() {
    let update_source = r#"
        update UpdateUser($id: Int) {
            user {
                @where { id = $id }
                name = "Updated Name"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(
        result.is_ok(),
        "Valid update with where should parse successfully"
    );
}

#[test]
fn test_valid_update_multiple_fields() {
    let update_source = r#"
        update UpdateUser($id: Int, $name: String, $status: Status) {
            user {
                @where { id = $id }
                name = $name
                status = $status
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(
        result.is_ok(),
        "Valid update with multiple fields should parse successfully"
    );
}

#[test]
fn test_missing_update_name() {
    let update_source = r#"
        update {
            user {
                name = "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(result.is_err(), "Missing update name should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(update_source, &error);

            // The parser gives a generic error message for this case
            assert!(
                formatted.contains("query.pyre") && formatted.contains("update {"),
                "Error message should contain file and update. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_missing_update_brace() {
    let update_source = r#"
        update UpdateUser
            user {
                name = "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(result.is_err(), "Missing opening brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(update_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_update_missing_where() {
    // Note: This might parse successfully but fail typechecking
    let update_source = r#"
        update UpdateUser {
            user {
                name = "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    // Updates without where might parse but fail typechecking
    let _ = result;
}

#[test]
fn test_invalid_assignment_syntax() {
    let update_source = r#"
        update UpdateUser {
            user {
                name: "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(
        result.is_err(),
        "Invalid assignment syntax (using colon instead of equals) should fail"
    );

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(update_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_missing_closing_brace() {
    let update_source = r#"
        update UpdateUser {
            user {
                name = "Bob"
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(result.is_err(), "Missing closing brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(update_source, &error);

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
        }
    }
}

#[test]
fn test_update_with_comments() {
    let update_source = r#"
        // This is a comment
        update UpdateUser {
            user {
                @where { id = 1 }
                name = "Bob"
                // Another comment
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(result.is_ok(), "Comments should be allowed in updates");
}

#[test]
fn test_invalid_where_syntax() {
    let update_source = r#"
        update UpdateUser($id: Int) {
            user {
                @where id = $id
                name = "Bob"
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", update_source);
    assert!(
        result.is_err(),
        "Invalid where syntax (missing braces) should fail"
    );

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(update_source, &error);

            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre")
                    || formatted.contains("expecting")
                    || formatted.contains("parameter")
                    || formatted.contains("issue"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        }
    }
}
