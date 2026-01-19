use pyre::ast;
use pyre::format;
use pyre::generate;
use pyre::parser;

fn create_test_database() -> ast::Database {
    let schema_source = r#"
record User {
    id Int @id
    name String
    email String
}

record Post {
    id Int @id
    title String
    content String?
    published Bool
    createdAt DateTime
    authorUserId Int
    users @link(authorUserId, User.id)
}

session {
    userId Int
}
    "#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).unwrap();
    ast::Database {
        schemas: vec![schema],
    }
}

/// Test helper to check newline behavior
fn check_newlines(source: &str, expected_start_newlines: usize, expected_end_newlines: usize) {
    let database = create_test_database();

    // Parse original
    let query_list = parser::parse_query("query.pyre", source);
    assert!(
        query_list.is_ok(),
        "Parse should succeed. Error: {:?}",
        query_list.err()
    );
    let mut query_list = query_list.unwrap();

    // Format
    format::query_list(&database, &mut query_list);
    let formatted = generate::to_string::query(&query_list);

    // Check start newlines
    let start_newlines = formatted.chars().take_while(|&c| c == '\n').count();
    assert_eq!(
        start_newlines, expected_start_newlines,
        "Expected {} newlines at start, got {}. Formatted:\n{}",
        expected_start_newlines, start_newlines, formatted
    );

    // Check end newlines
    let end_newlines = formatted.chars().rev().take_while(|&c| c == '\n').count();
    assert_eq!(
        end_newlines, expected_end_newlines,
        "Expected {} newlines at end, got {}. Formatted:\n{}",
        expected_end_newlines, end_newlines, formatted
    );
}

#[test]
fn test_single_query_no_leading_newline() {
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}"#;
    // Should have 0 newlines at start (no leading newline)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_single_query_with_leading_newline() {
    let source = r#"
query GetUsers {
    user {
        id
        name
    }
}"#;
    // Should have 0 newlines at start after formatting (leading newline removed)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_comment_at_start() {
    let source = r#"// Get all users
query GetUsers {
    user {
        id
        name
    }
}"#;
    // Should have 0 newlines at start (comment is first)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_comment_and_leading_newline() {
    let source = r#"
// Get all users
query GetUsers {
    user {
        id
        name
    }
}"#;
    // Should have 0 newlines at start (comment is first, leading newline removed)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_multiple_queries() {
    let source = r#"query GetUsers {
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
}"#;
    // Should have 0 newlines at start
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_multiple_queries_with_comments() {
    let source = r#"// First query
query GetUsers {
    user {
        id
        name
    }
}

// Second query
query GetPosts {
    post {
        id
        title
    }
}"#;
    // Should have 0 newlines at start (comment is first)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_trailing_newlines() {
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}


"#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (extra ones removed)
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_excessive_trailing_newlines() {
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}




"#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (excessive ones removed)
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_no_trailing_newlines() {
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}"#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (added if missing)
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_one_trailing_newline() {
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}
"#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (one added to make it 2)
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_leading_and_trailing_newlines() {
    let source = r#"

query GetUsers {
    user {
        id
        name
    }
}


"#;
    // Should have 0 newlines at start (leading newlines removed)
    // Should have exactly 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_only_comments() {
    let source = r#"// Comment 1
// Comment 2
"#;
    // Should have 0 newlines at start (comment is first)
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_empty_file() {
    let source = "";
    let database = create_test_database();

    // Empty file can't be parsed, so create an empty query list manually
    let mut query_list = ast::QueryList {
        queries: Vec::new(),
    };

    // Format
    format::query_list(&database, &mut query_list);
    let formatted = generate::to_string::query(&query_list);

    // Check end newlines
    let end_newlines = formatted.chars().rev().take_while(|&c| c == '\n').count();
    assert_eq!(
        end_newlines, 2,
        "Expected 2 newlines at end, got {}. Formatted:\n{:?}",
        end_newlines, formatted
    );
}

#[test]
fn test_query_with_args() {
    let source = r#"query GetUser($id: Int) {
    user {
        @where { id == $id }
        id
        name
    }
}"#;
    // Should have 0 newlines at start
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_query_with_limit_sort_where() {
    let source = r#"query GetUsers {
    user {
        @limit(10)
        @sort(name, Asc)
        @where { id == 1 }
        id
        name
    }
}"#;
    // Should have 0 newlines at start
    // Should have 2 newlines at end
    check_newlines(source, 0, 2);
}

#[test]
fn test_get_post_query_exact() {
    // This is the exact query from getPost.pyre that was failing
    let source = r#"query GetPost($id: Int) {
    post {
        @where { id == $id  }

        id
        title
        content
        published
        createdAt
        authorUserId
        users {
            id
            name
            email
        }
    }
}
    "#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (not 3)
    check_newlines(source, 0, 2);
}

#[test]
fn test_get_post_query_with_trailing_newlines() {
    // Same query but with 3 trailing newlines (the problematic case)
    let source = r#"query GetPost($id: Int) {
    post {
        @where { id == $id  }

        id
        title
        content
        published
        createdAt
        authorUserId
        users {
            id
            name
            email
        }
    }
}


"#;
    // Should have 0 newlines at start
    // Should have exactly 2 newlines at end (normalized from 3)
    check_newlines(source, 0, 2);
}

// Note: Update query tests removed because they require Post record in schema
// The round trip tests verify the newline behavior works correctly

#[test]
fn test_round_trip_update_post() {
    // Test that formatting multiple times doesn't keep adding newlines
    // This simulates the actual bug where formatting keeps adding newlines
    let database = create_test_database();

    // Use a working query format as a base
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}"#;

    // Parse and format first time
    let mut query_list1 = parser::parse_query("query.pyre", source).unwrap();
    format::query_list(&database, &mut query_list1);
    let formatted1 = generate::to_string::query(&query_list1);

    // Parse and format second time (should be stable - this is where the bug manifests)
    let mut query_list2 = parser::parse_query("query.pyre", &formatted1).unwrap();
    format::query_list(&database, &mut query_list2);
    let formatted2 = generate::to_string::query(&query_list2);

    // Parse and format third time (should still be stable)
    let mut query_list3 = parser::parse_query("query.pyre", &formatted2).unwrap();
    format::query_list(&database, &mut query_list3);
    let formatted3 = generate::to_string::query(&query_list3);

    // Check that all three formatted versions have exactly 2 trailing newlines
    let end_newlines1 = formatted1.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines2 = formatted2.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines3 = formatted3.chars().rev().take_while(|&c| c == '\n').count();

    assert_eq!(
        end_newlines1, 2,
        "First format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines1, formatted1
    );
    assert_eq!(
        end_newlines2, 2,
        "Second format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines2, formatted2
    );
    assert_eq!(
        end_newlines3, 2,
        "Third format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines3, formatted3
    );

    // Also check that formatted2 and formatted3 are the same (idempotent)
    assert_eq!(
        formatted2, formatted3,
        "Formatting should be idempotent. Second:\n{}\n\nThird:\n{}",
        formatted2, formatted3
    );
}

#[test]
fn test_round_trip_with_many_trailing_newlines() {
    // Test the specific bug: file with many trailing newlines that keeps growing
    let database = create_test_database();

    // Start with query that has many trailing newlines (simulating the actual file)
    let source = r#"query GetUsers {
    user {
        id
        name
    }
}




"#;

    // Parse and format first time
    let mut query_list1 = parser::parse_query("query.pyre", source).unwrap();
    format::query_list(&database, &mut query_list1);
    let formatted1 = generate::to_string::query(&query_list1);

    // Parse and format second time (should be stable - this is where the bug manifests)
    let mut query_list2 = parser::parse_query("query.pyre", &formatted1).unwrap();
    format::query_list(&database, &mut query_list2);
    let formatted2 = generate::to_string::query(&query_list2);

    // Parse and format third time (should still be stable)
    let mut query_list3 = parser::parse_query("query.pyre", &formatted2).unwrap();
    format::query_list(&database, &mut query_list3);
    let formatted3 = generate::to_string::query(&query_list3);

    // Check that all three formatted versions have exactly 2 trailing newlines
    let end_newlines1 = formatted1.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines2 = formatted2.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines3 = formatted3.chars().rev().take_while(|&c| c == '\n').count();

    assert_eq!(
        end_newlines1, 2,
        "First format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines1, formatted1
    );
    assert_eq!(
        end_newlines2, 2,
        "Second format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines2, formatted2
    );
    assert_eq!(
        end_newlines3, 2,
        "Third format should have 2 newlines, got {}. Formatted:\n{}",
        end_newlines3, formatted3
    );

    // Also check that formatted2 and formatted3 are the same (idempotent)
    assert_eq!(
        formatted2, formatted3,
        "Formatting should be idempotent. Second:\n{}\n\nThird:\n{}",
        formatted2, formatted3
    );
}

#[test]
fn test_update_post_query_round_trip() {
    // Test that formatting the updatePost query multiple times doesn't keep adding newlines
    let database = create_test_database();

    // This is the exact query from updatePost.pyre - start with exactly 2 newlines
    let source = r#"update UpdatePost($id: Int, $title: String?, $content: String?, $published: Bool?) {
    post {
        @where { id == $id }
        title = $title
        content = $content
        published = $published
    }
}

"#;

    // Parse and format first time
    let mut query_list1 = parser::parse_query("query.pyre", source).unwrap();
    format::query_list(&database, &mut query_list1);
    let formatted1 = generate::to_string::query(&query_list1);

    // Parse and format second time (should be stable - this is where the bug manifests)
    let mut query_list2 = parser::parse_query("query.pyre", &formatted1).unwrap();
    format::query_list(&database, &mut query_list2);
    let formatted2 = generate::to_string::query(&query_list2);

    // Parse and format third time (should still be stable)
    let mut query_list3 = parser::parse_query("query.pyre", &formatted2).unwrap();
    format::query_list(&database, &mut query_list3);
    let formatted3 = generate::to_string::query(&query_list3);

    // Check that all three formatted versions have exactly 2 trailing newlines
    let end_newlines1 = formatted1.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines2 = formatted2.chars().rev().take_while(|&c| c == '\n').count();
    let end_newlines3 = formatted3.chars().rev().take_while(|&c| c == '\n').count();

    assert_eq!(
        end_newlines1, 2,
        "First format should have 2 newlines, got {}. Formatted:\n{:?}",
        end_newlines1, formatted1
    );
    assert_eq!(
        end_newlines2, 2,
        "Second format should have 2 newlines, got {}. Formatted:\n{:?}",
        end_newlines2, formatted2
    );
    assert_eq!(
        end_newlines3, 2,
        "Third format should have 2 newlines, got {}. Formatted:\n{:?}",
        end_newlines3, formatted3
    );

    // Also check that formatted2 and formatted3 are the same (idempotent)
    assert_eq!(
        formatted2, formatted3,
        "Formatting should be idempotent. Second:\n{:?}\n\nThird:\n{:?}",
        formatted2, formatted3
    );

    // Also check that formatted1 equals formatted2 (should be idempotent from the start)
    assert_eq!(
        formatted1, formatted2,
        "Formatting should be idempotent from first format. First:\n{:?}\n\nSecond:\n{:?}",
        formatted1, formatted2
    );
}

#[test]
fn test_update_post_query_exact_from_file() {
    // This is the EXACT query from updatePost.pyre with all its trailing newlines
    // The file has 6 trailing newlines after the closing brace
    let source = r#"update UpdatePost($id: Int, $title: String?, $content: String?, $published: Bool?) {
    post {
        @where { id == $id }
        title = $title
        content = $content
        published = $published
    }
}




"#;
    let database = create_test_database();

    // Parse and format
    let mut query_list = parser::parse_query("query.pyre", source).unwrap();
    format::query_list(&database, &mut query_list);
    let formatted = generate::to_string::query(&query_list);

    // Count trailing newlines
    let end_newlines = formatted.chars().rev().take_while(|&c| c == '\n').count();

    // Should have exactly 2 newlines, not 3
    assert_eq!(
        end_newlines, 2,
        "Formatted query should have exactly 2 trailing newlines, got {}. Formatted:\n{:?}",
        end_newlines, formatted
    );

    // Now format again - should still have 2 newlines
    let mut query_list2 = parser::parse_query("query.pyre", &formatted).unwrap();
    format::query_list(&database, &mut query_list2);
    let formatted2 = generate::to_string::query(&query_list2);

    let end_newlines2 = formatted2.chars().rev().take_while(|&c| c == '\n').count();

    assert_eq!(
        end_newlines2, 2,
        "Second format should still have exactly 2 trailing newlines, got {}. Formatted:\n{:?}",
        end_newlines2, formatted2
    );

    // The two formatted versions should be identical
    assert_eq!(
        formatted, formatted2,
        "Formatting should be idempotent. First:\n{:?}\n\nSecond:\n{:?}",
        formatted, formatted2
    );
}
