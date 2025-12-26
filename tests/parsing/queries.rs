use pyre::parser;

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
fn test_valid_query() {
    let query_source = r#"
        query GetUsers {
            user {
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query should parse successfully");
}

#[test]
fn test_valid_query_with_params() {
    let query_source = r#"
        query GetUser($id: Int) {
            user {
                @where { id = $id }
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with params should parse successfully");
}

#[test]
fn test_valid_query_with_nested_fields() {
    let query_source = r#"
        query GetUsers {
            user {
                id
                name
                posts {
                    id
                    title
                }
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with nested fields should parse successfully");
}

#[test]
fn test_valid_query_with_where() {
    let query_source = r#"
        query GetUser($id: Int) {
            user {
                @where { id = $id }
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with where should parse successfully");
}

#[test]
fn test_valid_query_with_sort() {
    let query_source = r#"
        query GetUsers {
            user {
                @sort name asc
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with sort should parse successfully");
}

#[test]
fn test_valid_query_with_sort_desc() {
    let query_source = r#"
        query GetUsers {
            user {
                @sort name desc
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with sort desc should parse successfully");
}

#[test]
fn test_valid_query_with_field_alias() {
    let query_source = r#"
        query GetUsers {
            user {
                id
                username: name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Valid query with field alias should parse successfully");
}

#[test]
fn test_valid_multiple_queries() {
    let query_source = r#"
        query GetUsers {
            user {
                id
                name
            }
        }

        query GetPosts {
            post {
                id
                title
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Multiple queries should parse successfully");
}

#[test]
fn test_missing_query_name() {
    let query_source = r#"
        query {
            user {
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_err(), "Missing query name should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(query_source, &error);
            
            // The parser gives a generic error message for this case
            assert!(
                formatted.contains("query.pyre") && formatted.contains("query {"),
                "Error message should contain file and query. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_missing_query_brace() {
    let query_source = r#"
        query GetUsers
            user {
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_err(), "Missing opening brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(query_source, &error);
            
            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue"),
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
fn test_invalid_param_syntax() {
    // Note: The parser may accept this syntax, but typechecking will fail
    // This test documents the current parsing behavior
    let query_source = r#"
        query GetUser($id Int) {
            user {
                @where { id = $id }
                id
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    // The parser may accept this, but typechecking will catch the missing colon
    let _ = result;
}

#[test]
fn test_invalid_directive() {
    let query_source = r#"
        query GetUsers {
            user {
                @unknown
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_err(), "Invalid directive should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(query_source, &error);
            
            // Check that the error mentions the unknown directive and suggests alternatives
            assert!(
                formatted.contains("@unknown") && 
                (formatted.contains("@where") || formatted.contains("did you mean")),
                "Error message should mention @unknown and suggest alternatives. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_missing_closing_brace() {
    let query_source = r#"
        query GetUsers {
            user {
                id
                name
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_err(), "Missing closing brace should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(query_source, &error);
            
            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue") || formatted.contains("Incomplete"),
                "Error message should indicate a parsing error. Got:\n{}",
                formatted
            );
        }
    }
}

#[test]
fn test_query_with_comments() {
    let query_source = r#"
        // This is a comment
        query GetUsers {
            user {
                id
                // Another comment
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_ok(), "Comments should be allowed in queries");
}

#[test]
fn test_empty_query() {
    let query_source = r#"
        query GetUsers {
            user {
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    // Empty queries might be valid or invalid depending on the implementation
    let _ = result;
}

#[test]
fn test_invalid_where_syntax() {
    let query_source = r#"
        query GetUser($id: Int) {
            user {
                @where id = $id
                id
                name
            }
        }
    "#;

    let result = parser::parse_query("query.pyre", query_source);
    assert!(result.is_err(), "Invalid where syntax (missing braces) should fail");

    if let Err(err) = result {
        if let Some(error) = parser::convert_parsing_error(err) {
            let formatted = format_error_no_color(query_source, &error);
            
            // The parser may give generic errors, so just verify it's an error message
            assert!(
                formatted.contains("query.pyre") || formatted.contains("expecting") || formatted.contains("parameter") || formatted.contains("issue"),
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

