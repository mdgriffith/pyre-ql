use crate::helpers::{TestDatabase, TestError};
use std::collections::HashMap;

#[tokio::test]
async fn test_one_to_many_relationship() -> Result<(), TestError> {
    let schema = r#"
        record User {
            id   Int    @id
            name String
            posts @link(Post.authorId)
        }

        record Post {
            id        Int    @id
            title     String
            content   String
            authorId  Int
            author    @link(authorId, User.id)
        }
    "#;

    let db = TestDatabase::new(schema).await?;

    // Seed data using pyre insert queries
    // First insert the user
    let insert_user = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    db.execute_insert_with_params(insert_user, params).await?;

    // Then insert posts - we need to get the user ID first
    // For now, we'll insert posts with authorId = 1 (assuming auto-increment starts at 1)
    let insert_post1 = r#"
        insert CreatePost($title: String, $content: String, $authorId: Int) {
            post {
                title = $title
                content = $content
                authorId = $authorId
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert(
        "title".to_string(),
        libsql::Value::Text("First Post".to_string()),
    );
    params.insert(
        "content".to_string(),
        libsql::Value::Text("Content here".to_string()),
    );
    params.insert("authorId".to_string(), libsql::Value::Integer(1));
    db.execute_insert_with_params(insert_post1, params).await?;

    let mut params = HashMap::new();
    params.insert(
        "title".to_string(),
        libsql::Value::Text("Second Post".to_string()),
    );
    params.insert(
        "content".to_string(),
        libsql::Value::Text("More content".to_string()),
    );
    params.insert("authorId".to_string(), libsql::Value::Integer(1));
    db.execute_insert_with_params(insert_post1, params).await?;

    // Query user with posts
    let query = r#"
        query GetUserWithPosts {
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

    // Execute the query and check results
    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify we got results
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );

    let users = results.get("user").unwrap();
    assert_eq!(users.len(), 1, "Should have 1 user");

    let user = &users[0];
    assert_eq!(
        user.get("name").and_then(|n| n.as_str()),
        Some("Alice"),
        "User name should be Alice"
    );

    // Check that posts are included
    assert!(user.get("posts").is_some(), "User should have posts field");

    let posts = user.get("posts").and_then(|p| p.as_array()).unwrap();
    assert_eq!(posts.len(), 2, "User should have 2 posts");

    let post_titles: Vec<&str> = posts
        .iter()
        .filter_map(|p| p.get("title").and_then(|t| t.as_str()))
        .collect();
    assert!(
        post_titles.contains(&"First Post"),
        "Should contain 'First Post'"
    );
    assert!(
        post_titles.contains(&"Second Post"),
        "Should contain 'Second Post'"
    );

    Ok(())
}
