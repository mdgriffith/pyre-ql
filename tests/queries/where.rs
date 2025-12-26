use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;
use std::collections::HashMap;

#[tokio::test]
async fn test_where_clause_filtering() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUserByName($name: String) {
            user {
                @where { name = $name }
                id
                name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));

    let rows = db.execute_query_with_params(query, params).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    assert_eq!(users.len(), 1, "Should have exactly 1 user");
    assert_eq!(
        users[0].get("name").and_then(|n| n.as_str()),
        Some("Alice"),
        "Should return Alice"
    );

    Ok(())
}

#[tokio::test]
async fn test_where_clause_with_status() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetActiveUsers($status: Status) {
            user {
                @where { status = $status }
                id
                name
                status
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Active".to_string()),
    );

    let rows = db.execute_query_with_params(query, params).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();

    // Verify all returned users have Active status
    for user in users {
        assert_eq!(
            user.get("status").and_then(|s| s.as_str()),
            Some("Active"),
            "All users should have Active status"
        );
    }

    Ok(())
}

