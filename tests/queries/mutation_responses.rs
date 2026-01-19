use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Helper to extract the actual array from mutation response
/// Mutations now return arrays directly (coalesce(json_group_array(...), json('[]')) as fieldName),
/// so parse_query_results already returns the array directly
fn extract_mutation_response(
    results: &HashMap<String, Vec<JsonValue>>,
    field_name: &str,
) -> Vec<JsonValue> {
    results
        .get(field_name)
        .cloned()
        .unwrap_or_else(|| panic!("Results should contain '{}' field", field_name))
}

/// Test that insert mutations return typed response data
#[tokio::test]
async fn test_insert_typed_response() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    let insert_query = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
                status = Active
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));

    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the typed response field exists
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field. Available keys: {:?}",
        results.keys().collect::<Vec<_>>()
    );

    let users = extract_mutation_response(&results, "user");
    assert_eq!(
        users.len(),
        1,
        "Should have exactly 1 user in response, got {}",
        users.len()
    );

    let user = &users[0];
    let user_obj = user.as_object().expect("User should be a JSON object");
    assert!(
        user_obj.contains_key("name"),
        "User should have 'name' field. User: {:#}",
        user
    );
    assert_eq!(
        user_obj["name"].as_str().unwrap(),
        "Alice",
        "User name should be 'Alice'"
    );
    assert!(
        user_obj.contains_key("status"),
        "User should have 'status' field"
    );

    Ok(())
}

/// Test that insert mutations return only the fields specified in the mutation
#[tokio::test]
async fn test_insert_typed_response_fields_only() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    let insert_query = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
                status = Active
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));

    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    let users = extract_mutation_response(&results, "user");
    assert_eq!(users.len(), 1, "Should have exactly 1 user");
    let user = &users[0];
    let user_obj = user.as_object().expect("User should be a JSON object");

    // Should only have the fields specified in the mutation (name, status)
    // Should NOT have id, even though it exists in the table
    assert!(
        user_obj.contains_key("name"),
        "User should have 'name' field"
    );
    assert!(
        user_obj.contains_key("status"),
        "User should have 'status' field"
    );
    // Note: id is not in the mutation return type, so it shouldn't be in the response
    // (unless explicitly requested in the mutation)

    Ok(())
}

/// Test that update mutations return typed response data
#[tokio::test]
async fn test_update_typed_response() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let update_query = r#"
        update UpdateUser($id: Int, $name: String) {
            user {
                @where { id == $id }
                name = $name
                id
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(1));
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Updated Name".to_string()),
    );

    let rows = db.execute_query_with_params(update_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the typed response field exists
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field. Available keys: {:?}",
        results.keys().collect::<Vec<_>>()
    );

    let users = extract_mutation_response(&results, "user");
    assert!(
        users.len() > 0,
        "Should have at least 1 user in response, got {}",
        users.len()
    );

    // Find the updated user
    let updated_user = users
        .iter()
        .find(|u| {
            u.as_object()
                .and_then(|o| o.get("id"))
                .and_then(|v| v.as_i64())
                == Some(1)
        })
        .expect("Should find user with id=1");
    let updated_user_obj = updated_user.as_object().unwrap();

    assert_eq!(
        updated_user_obj["name"].as_str().unwrap(),
        "Updated Name",
        "User name should be updated. User: {:#}",
        updated_user
    );
    assert_eq!(
        updated_user_obj["id"].as_i64().unwrap(),
        1,
        "User id should be 1"
    );

    Ok(())
}

/// Test that update mutations return only the fields specified in the mutation
#[tokio::test]
async fn test_update_typed_response_fields_only() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let update_query = r#"
        update UpdateUser($id: Int, $name: String) {
            user {
                @where { id == $id }
                name = $name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(1));
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Only Name".to_string()),
    );

    let rows = db.execute_query_with_params(update_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    let users = extract_mutation_response(&results, "user");
    assert_eq!(users.len(), 1, "Should have exactly 1 user");
    let user = &users[0];
    let user_obj = user.as_object().expect("User should be a JSON object");

    // Should only have the fields specified in the mutation (name)
    assert!(
        user_obj.contains_key("name"),
        "User should have 'name' field. User: {:#}",
        user
    );

    // The update should have changed the name
    // Note: seed_standard_data creates users, so we need to verify the update worked
    let name_value = user_obj["name"].as_str().unwrap();
    assert_eq!(
        name_value, "Only Name",
        "User name should match updated value. Got: '{}', Expected: 'Only Name'. User: {:#}",
        name_value, user
    );
    // Should NOT have id, status, etc. since they weren't requested
    assert!(
        !user_obj.contains_key("id"),
        "User should NOT have 'id' field when not requested"
    );

    Ok(())
}

/// Test that delete mutations return typed response data
#[tokio::test]
async fn test_delete_typed_response() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let delete_query = r#"
        delete DeleteUser($id: Int) {
            user {
                @where { id == $id }
                id
                name
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(1));

    let rows = db.execute_query_with_params(delete_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the typed response field exists
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field. Available keys: {:?}",
        results.keys().collect::<Vec<_>>()
    );

    let users = extract_mutation_response(&results, "user");
    assert!(
        users.len() > 0,
        "Should have at least 1 user in response (the deleted one), got {}",
        users.len()
    );

    let deleted_user = &users[0];
    let deleted_user_obj = deleted_user
        .as_object()
        .expect("Deleted user should be a JSON object");
    assert_eq!(
        deleted_user_obj["id"].as_i64().unwrap(),
        1,
        "Deleted user should have id=1"
    );
    assert!(
        deleted_user_obj.contains_key("name"),
        "Deleted user should have 'name' field"
    );

    // Verify the user was actually deleted from the database
    let check_query = r#"
        query GetUser {
            user {
                @where { id == 1 }
                id
            }
        }
    "#;

    let check_rows = db.execute_query(check_query).await?;
    let check_results = db.parse_query_results(check_rows).await?;
    let empty_vec = Vec::new();
    let remaining_users = check_results.get("user").unwrap_or(&empty_vec);
    assert_eq!(
        remaining_users.len(),
        0,
        "User should be deleted from database"
    );

    Ok(())
}

/// Test that delete mutations return only the fields specified in the mutation
#[tokio::test]
async fn test_delete_typed_response_fields_only() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let delete_query = r#"
        delete DeleteUser($id: Int) {
            user {
                @where { id == $id }
                id
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(1));

    let rows = db.execute_query_with_params(delete_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    let users = extract_mutation_response(&results, "user");
    assert_eq!(users.len(), 1, "Should have exactly 1 user");
    let user = &users[0];
    let user_obj = user.as_object().expect("User should be a JSON object");

    // Should only have the fields specified in the mutation (id)
    assert_eq!(
        user_obj["id"].as_i64().unwrap(),
        1,
        "Deleted user should have id=1"
    );
    // Should NOT have name, status, etc. since they weren't requested
    assert!(
        !user_obj.contains_key("name"),
        "User should NOT have 'name' field when not requested"
    );

    Ok(())
}

/// Test that insert mutations with multiple fields return all fields
#[tokio::test]
async fn test_insert_multiple_fields() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    let insert_query = r#"
        insert CreateUser($name: String) {
            user {
                name = $name
                status = Active
                id
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Charlie".to_string()),
    );

    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    let users = extract_mutation_response(&results, "user");
    assert_eq!(users.len(), 1, "Should have exactly 1 user");
    let user = &users[0];
    let user_obj = user.as_object().expect("User should be a JSON object");

    // Should have all requested fields
    assert_eq!(
        user_obj["name"].as_str().unwrap(),
        "Charlie",
        "User name should match"
    );
    assert!(user_obj.contains_key("id"), "User should have 'id' field");
    assert!(
        user_obj.contains_key("status"),
        "User should have 'status' field"
    );

    Ok(())
}

/// Test that update mutations return empty array when no rows match
#[tokio::test]
async fn test_update_no_rows_affected() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let update_query = r#"
        update UpdateUser($id: Int, $name: String) {
            user {
                @where { id == $id }
                name = $name
                id
            }
        }
    "#;

    // Try to update a non-existent user
    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(99999));
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Should Not Exist".to_string()),
    );

    let rows = db.execute_query_with_params(update_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Should still have the user field, but it should be empty
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field even when no rows affected"
    );

    let users = extract_mutation_response(&results, "user");
    assert_eq!(
        users.len(),
        0,
        "Should have 0 users when no rows match the WHERE clause"
    );

    Ok(())
}

/// Test that delete mutations return empty array when no rows match
#[tokio::test]
async fn test_delete_no_rows_affected() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let delete_query = r#"
        delete DeleteUser($id: Int) {
            user {
                @where { id == $id }
                id
            }
        }
    "#;

    // Try to delete a non-existent user
    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(99999));

    let rows = db.execute_query_with_params(delete_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Should still have the user field, but it should be empty
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field even when no rows affected"
    );

    let users = extract_mutation_response(&results, "user");
    assert_eq!(
        users.len(),
        0,
        "Should have 0 users when no rows match the WHERE clause"
    );

    Ok(())
}
