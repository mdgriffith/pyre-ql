#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{TestDatabase, TestError};
use std::collections::HashMap;

#[tokio::test]
async fn test_basic_schema_and_query() -> Result<(), TestError> {
    let schema = r#"
        record User {
            id   Int    @id
            name String
        }
    "#;

    let db = TestDatabase::new(schema).await?;

    // Seed data using pyre insert queries
    let insert_alice = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let insert_bob = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    db.execute_insert_with_params(insert_alice, params).await?;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
    db.execute_insert_with_params(insert_bob, params).await?;

    // Query users - query field name is decapitalized: "User" -> "user"
    let query = r#"
        query GetUsers {
            user {
                id
                name
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
    assert_eq!(users.len(), 2, "Should have 2 users");

    // Check that we have Alice and Bob
    let names: Vec<String> = users
        .iter()
        .filter_map(|u| {
            u.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(names.contains(&"Alice".to_string()), "Should contain Alice");
    assert!(names.contains(&"Bob".to_string()), "Should contain Bob");

    Ok(())
}

#[tokio::test]
async fn test_insert_query() -> Result<(), TestError> {
    let schema = r#"
        record User {
            id   Int    @id
            name String
        }
    "#;

    let db = TestDatabase::new(schema).await?;

    // Insert a user via query
    let insert_query = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    // Execute with a parameter
    let mut params = HashMap::new();
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Charlie".to_string()),
    );
    db.execute_insert_with_params(insert_query, params).await?;

    // Verify the insert worked by querying the data
    let query = r#"
        query GetUsers {
            user {
                id
                name
            }
        }
    "#;

    let query_rows = db.execute_query(query).await?;
    let results = db.parse_query_results(query_rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );

    let users = results.get("user").unwrap();
    assert_eq!(users.len(), 1, "Should have 1 user after insert");

    let user_name = users[0].get("name").and_then(|n| n.as_str()).unwrap();
    assert_eq!(user_name, "Charlie", "Inserted user should be Charlie");

    Ok(())
}

#[tokio::test]
async fn test_where_clause() -> Result<(), TestError> {
    let schema = r#"
        record User {
            id   Int    @id
            name String
        }
    "#;

    let db = TestDatabase::new(schema).await?;

    // Seed data using pyre insert queries
    let insert_alice = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let insert_bob = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    db.execute_insert_with_params(insert_alice, params).await?;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
    db.execute_insert_with_params(insert_bob, params).await?;

    // Query with where clause - @where goes on its own line inside the field block
    let query = r#"
        query GetUser($name: String) {
            user {
                @where { name = $name }
                id
                name
            }
        }
    "#;

    // Execute query with parameter
    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    let rows = db.execute_query_with_params(query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify we got the correct user
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );

    let users = results.get("user").unwrap();
    assert_eq!(
        users.len(),
        1,
        "Should have 1 user matching the where clause"
    );

    let user_name = users[0].get("name").and_then(|n| n.as_str()).unwrap();
    assert_eq!(user_name, "Alice", "Should return Alice");

    Ok(())
}
