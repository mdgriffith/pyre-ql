//! Tests for union types in Pyre schemas.
//!
//! Union types in Pyre have several important characteristics:
//! - Variants can have sub-records with fields
//! - If multiple variants have fields with the same name, those fields *must* have the same type
//! - When field names match across variants, they share the same database column
//! - This column sharing enables efficient storage and querying of union types

use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;

#[tokio::test]
async fn test_union_type_in_schema() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

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

/// Test that union variants with matching field names and types share the same column
///
/// When multiple variants have fields with the same name and type, they share a single
/// database column. This test verifies that Success and Warning variants both use
/// the same `result__message` column for their `message String` fields.
#[tokio::test]
async fn test_union_column_reuse() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::union_column_reuse_schema()).await?;

    // Insert records with different variants that share the same field name and type
    let insert_success = r#"
        insert CreateTestRecord($message: String) {
            testRecord {
                id = 1
                result = Success { message = $message }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert(
        "message".to_string(),
        libsql::Value::Text("Success!".to_string()),
    );
    db.execute_insert_with_params(insert_success, params)
        .await?;

    let insert_warning = r#"
        insert CreateTestRecord($message: String) {
            testRecord {
                id = 2
                result = Warning { message = $message }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert(
        "message".to_string(),
        libsql::Value::Text("Warning!".to_string()),
    );
    db.execute_insert_with_params(insert_warning, params)
        .await?;

    // Query the records to verify they were stored correctly
    let query = r#"
        query GetTests {
            testRecord {
                id
                result
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("testRecord"),
        "Results should contain 'testrecord' field"
    );

    let records = results.get("testRecord").unwrap();
    assert_eq!(records.len(), 2, "Should have 2 test records");

    // Check that both Success and Warning variants were stored
    let result_values: Vec<&str> = records
        .iter()
        .filter_map(|r| r.get("result").and_then(|s| s.as_str()))
        .collect();
    assert!(
        result_values.contains(&"Success"),
        "Should contain 'Success' variant"
    );
    assert!(
        result_values.contains(&"Warning"),
        "Should contain 'Warning' variant"
    );

    // Verify that the message column is shared by checking the database schema
    // Both Success and Warning should use the same column: result__message
    let schema_query = "PRAGMA table_info(testRecords)";
    let mut schema_rows = db.execute_raw(schema_query).await?;
    let mut column_names = Vec::new();
    while let Some(row) = schema_rows.next().await.map_err(TestError::Database)? {
        if let Ok(col_name) = row.get::<String>(1) {
            column_names.push(col_name);
        }
    }

    // Should have: id, result (discriminator), and result__message (shared column)
    assert!(
        column_names.contains(&"result__message".to_string()),
        "Should have shared column 'result__message'. Columns: {:?}",
        column_names
    );
    // Should NOT have separate columns for each variant
    assert!(
        !column_names.contains(&"result__Success__message".to_string()),
        "Should not have variant-specific column. Columns: {:?}",
        column_names
    );
    assert!(
        !column_names.contains(&"result__Warning__message".to_string()),
        "Should not have variant-specific column. Columns: {:?}",
        column_names
    );

    Ok(())
}

/// Test that union variants with matching field names but different types are rejected
///
/// According to the union type rules, if field names match across variants, their types
/// must also match. This test verifies that schemas violating this rule are rejected.
#[tokio::test]
async fn test_union_type_collision_rejected() -> Result<(), TestError> {
    // Attempting to create a database with a schema that has matching field names
    // but different types should fail validation
    let result = TestDatabase::new(&schema::union_separate_columns_schema()).await;

    assert!(
        result.is_err(),
        "Schema with matching field names but different types should be rejected"
    );

    // Verify the error message indicates a type collision
    if let Err(e) = result {
        let error_str = format!("{}", e);
        assert!(
            error_str.contains("Fields with the same name across variants must have the same type")
                || error_str.contains("Variant Field Type Collision")
                || error_str.contains("type collision")
                || error_str.contains("collision"),
            "Expected type collision error, got: {}",
            error_str
        );
    }

    Ok(())
}

/// Test that sub-records are required by default - validation should fail if required fields are missing
#[tokio::test]
async fn test_union_required_fields_validation() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::union_required_fields_schema()).await?;

    // Test 1: Insert with Simple variant (no sub-record) - should succeed
    let insert_simple = r#"
        insert CreateTestRecord {
            testRecord {
                id = 1
                action = Simple
            }
        }
    "#;

    db.execute_insert_with_params(insert_simple, std::collections::HashMap::new())
        .await?;

    // Test 2: Insert with Create variant providing all required fields - should succeed
    let insert_create = r#"
        insert CreateTestRecord($name: String, $description: String) {
            testRecord {
                id = 2
                action = Create { name = $name, description = $description }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Test".to_string()));
    params.insert(
        "description".to_string(),
        libsql::Value::Text("Test description".to_string()),
    );
    db.execute_insert_with_params(insert_create, params).await?;

    // Test 3: Insert with Create variant missing required fields - should fail validation
    // This test verifies that the typechecker/validator catches missing required fields
    let insert_create_incomplete = r#"
        insert CreateTestRecord($name: String) {
            testRecord {
                id = 3
                action = Create { name = $name }
                // Missing description field - should fail
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Test".to_string()));

    // This should fail during typechecking/validation
    let result = db
        .execute_insert_with_params(insert_create_incomplete, params)
        .await;
    assert!(
        result.is_err(),
        "Insert with missing required field should fail validation"
    );

    // Test 4: Insert with Update variant providing all required fields - should succeed
    let insert_update = r#"
        insert CreateTestRecord($id: Int, $changes: String) {
            testRecord {
                id = 4
                action = Update { id = $id, changes = $changes }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(10));
    params.insert(
        "changes".to_string(),
        libsql::Value::Text("Updated".to_string()),
    );
    db.execute_insert_with_params(insert_update, params).await?;

    // Test 5: Insert with Delete variant missing required fields - should fail
    let insert_delete_incomplete = r#"
        insert CreateTestRecord($id: Int) {
            testRecord {
                id = 5
                action = Delete { id = $id }
                // Missing reason field - should fail
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(20));

    let result = db
        .execute_insert_with_params(insert_delete_incomplete, params)
        .await;
    assert!(
        result.is_err(),
        "Insert with missing required field should fail validation"
    );

    // Verify successful inserts were stored
    let query = r#"
        query GetTests {
            testRecord {
                id
                action
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("testRecord"),
        "Results should contain 'testrecord' field"
    );

    let records = results.get("testRecord").unwrap();
    // Should have 3 successful inserts (Simple, Create with all fields, Update with all fields)
    assert_eq!(
        records.len(),
        3,
        "Should have 3 successfully inserted records"
    );

    Ok(())
}
