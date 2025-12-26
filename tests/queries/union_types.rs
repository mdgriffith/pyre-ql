#[path = "../helpers/mod.rs"]
mod helpers;

use helpers::{TestDatabase, TestError};
use std::collections::HashMap;

#[tokio::test]
async fn test_union_type_in_schema() -> Result<(), TestError> {
    let schema = r#"
        record User {
            id     Int    @id
            name   String
            status Status
        }

        type Status
           = Active
           | Inactive
           | Special {
                reason String
             }
    "#;

    let db = TestDatabase::new(schema).await?;

    // Seed data using pyre insert queries with different status values
    let insert_alice = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Active".to_string()),
    );
    db.execute_insert_with_params(insert_alice, params).await?;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Inactive".to_string()),
    );
    db.execute_insert_with_params(insert_alice, params).await?;

    let mut params = HashMap::new();
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Charlie".to_string()),
    );
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Special".to_string()),
    );
    db.execute_insert_with_params(insert_alice, params).await?;

    // Query users with status
    let query = r#"
        query GetUsers {
            user {
                id
                name
                status
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
    assert_eq!(users.len(), 3, "Should have 3 users");

    // Check that all users have status fields
    for user in users {
        assert!(
            user.get("status").is_some(),
            "Each user should have a status field"
        );
    }

    // Verify the status values
    let statuses: Vec<&str> = users
        .iter()
        .filter_map(|u| u.get("status").and_then(|s| s.as_str()))
        .collect();
    assert!(
        statuses.contains(&"Active"),
        "Should contain 'Active' status"
    );
    assert!(
        statuses.contains(&"Inactive"),
        "Should contain 'Inactive' status"
    );
    assert!(
        statuses.contains(&"Special"),
        "Should contain 'Special' status"
    );

    Ok(())
}
