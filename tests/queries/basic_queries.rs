#[path = "../helpers/mod.rs"]
mod helpers;

use helpers::{schema, TestDatabase, TestError};

#[tokio::test]
async fn test_simple_query() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                id
                name
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field. Available keys: {:?}",
        results.keys().collect::<Vec<_>>()
    );
    let users = results.get("user").unwrap();
    assert!(
        users.len() >= 2,
        "Should have at least 2 users, but got {}. Users: {:#}",
        users.len(),
        serde_json::json!(users)
    );

    Ok(())
}

#[tokio::test]
async fn test_query_with_fields() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                id
                name
                status
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();

    // Verify all users have the requested fields
    for user in users {
        assert!(user.get("id").is_some(), "User should have id field");
        assert!(user.get("name").is_some(), "User should have name field");
        assert!(
            user.get("status").is_some(),
            "User should have status field"
        );
    }

    Ok(())
}
