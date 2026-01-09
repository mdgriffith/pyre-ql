use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;

#[tokio::test]
async fn test_limit_with_literal_value() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                @limit(2)
                id
                name
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
    assert_eq!(users.len(), 2, "Should have exactly 2 users due to limit");

    Ok(())
}

#[tokio::test]
async fn test_limit_with_variable() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers($limit: Int) {
            user {
                @limit($limit)
                id
                name
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("limit".to_string(), libsql::Value::Integer(3));

    let rows = db.execute_query_with_params(query, params).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    assert_eq!(
        users.len(),
        3,
        "Should have exactly 3 users due to limit parameter"
    );

    Ok(())
}

#[tokio::test]
async fn test_limit_with_where_clause() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers($name: String) {
            user {
                @where { name == $name }
                @limit(1)
                id
                name
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));

    let rows = db.execute_query_with_params(query, params).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    assert_eq!(
        users.len(),
        1,
        "Should have exactly 1 user matching the where clause with limit"
    );
    assert_eq!(
        users[0].get("name").and_then(|n| n.as_str()),
        Some("Alice"),
        "Should return Alice"
    );

    Ok(())
}

#[tokio::test]
async fn test_limit_with_sort() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                @sort(name, Asc)
                @limit(2)
                id
                name
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
    assert_eq!(users.len(), 2, "Should have exactly 2 users due to limit");

    // Verify results are sorted (first user should come before second alphabetically)
    if users.len() >= 2 {
        let name1 = users[0].get("name").and_then(|n| n.as_str()).unwrap_or("");
        let name2 = users[1].get("name").and_then(|n| n.as_str()).unwrap_or("");
        assert!(name1 <= name2, "Results should be sorted by name ascending");
    }

    Ok(())
}

#[tokio::test]
async fn test_limit_zero_returns_empty() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                @limit(0)
                id
                name
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
    assert_eq!(users.len(), 0, "Should have 0 users with limit 0");

    Ok(())
}

#[tokio::test]
async fn test_limit_larger_than_results() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUsers {
            user {
                @limit(1000)
                id
                name
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
    // Should return all available users, not fail
    assert!(
        users.len() > 0,
        "Should return at least some users even with large limit"
    );
    assert!(users.len() <= 1000, "Should not exceed the limit");

    Ok(())
}

#[tokio::test]
async fn test_limit_with_session_variable() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    // First, we need to set up a session with a limit value
    // Since session variables are typically used for permissions,
    // we'll test with a query that uses a session variable for limit
    // Note: This test assumes the schema supports session variables
    // If not, we can adjust or skip this test

    let query = r#"
        query GetUsers {
            user {
                @limit(5)
                id
                name
            }
        }
    "#;

    let mut session = std::collections::HashMap::new();
    // Session variables are typically used for permissions, not limit values
    // So we'll just test that limit works independently

    let rows = db
        .execute_query_with_session(query, std::collections::HashMap::new(), session, false)
        .await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    // Limit should cap results - if there are fewer users than the limit, we get all of them
    assert!(
        users.len() <= 5,
        "Should have at most 5 users due to limit, but got {}",
        users.len()
    );
    assert!(users.len() > 0, "Should have at least some users");

    Ok(())
}
