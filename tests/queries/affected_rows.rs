use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;
use std::collections::HashMap;

#[tokio::test]
async fn test_insert_affected_rows() -> Result<(), TestError> {
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

    // Check that we have result sets
    assert!(!rows.is_empty(), "Should have at least one result set");

    // The last result set should be _affectedRows
    let mut affected_rows_found = false;
    for mut rows_set in rows {
        let column_count = rows_set.column_count();
        for i in 0..column_count {
            if let Some(col_name) = rows_set.column_name(i) {
                if col_name == "_affectedRows" {
                    affected_rows_found = true;
                    // Get the first row
                    if let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                        if let Ok(json_str) = row.get::<String>(i as i32) {
                            let json_value: serde_json::Value = serde_json::from_str(&json_str)
                                .map_err(|e| {
                                    TestError::TypecheckError(format!(
                                        "Failed to parse JSON: {}",
                                        e
                                    ))
                                })?;

                            // Verify structure: should be an array
                            assert!(
                                json_value.is_array(),
                                "_affectedRows should be an array, got: {:#}",
                                json_value
                            );

                            let arr = json_value.as_array().unwrap();
                            assert!(
                                arr.len() > 0,
                                "Should have at least one affected row, got: {:#}",
                                json_value
                            );

                            // Verify each affected row has the correct structure
                            for (idx, affected_row) in arr.iter().enumerate() {
                                // Parse the affected row (it might be a JSON string)
                                let obj = if affected_row.is_string() {
                                    // If it's a string, parse it as JSON
                                    let row_str = affected_row.as_str().unwrap();
                                    serde_json::from_str::<serde_json::Value>(row_str)
                                        .map_err(|e| {
                                            TestError::TypecheckError(format!(
                                                "Failed to parse affected row JSON: {}",
                                                e
                                            ))
                                        })?
                                        .as_object()
                                        .ok_or_else(|| {
                                            TestError::TypecheckError(
                                                "Parsed affected row is not an object".to_string(),
                                            )
                                        })?
                                        .clone()
                                } else {
                                    assert!(
                                        affected_row.is_object(),
                                        "Each affected row should be an object or JSON string, got at index {}: {:#}",
                                        idx,
                                        affected_row
                                    );
                                    affected_row.as_object().unwrap().clone()
                                };

                                // Check for required fields
                                assert!(
                                    obj.contains_key("table_name"),
                                    "Affected row should have 'table_name' field"
                                );
                                assert!(
                                    obj.contains_key("row"),
                                    "Affected row should have 'row' field"
                                );
                                assert!(
                                    obj.contains_key("headers"),
                                    "Affected row should have 'headers' field"
                                );

                                // Verify table_name is a string
                                assert!(
                                    obj["table_name"].is_string(),
                                    "table_name should be a string"
                                );

                                // Verify row is an object
                                assert!(obj["row"].is_object(), "row should be an object");

                                // Verify headers is an array
                                assert!(obj["headers"].is_array(), "headers should be an array");

                                // Verify the row contains all columns
                                let row_obj = obj
                                    .get("row")
                                    .and_then(|v| v.as_object())
                                    .ok_or_else(|| {
                                        TestError::TypecheckError(
                                            "Row field should be an object".to_string(),
                                        )
                                    })?;
                                let headers_arr = obj
                                    .get("headers")
                                    .and_then(|v| v.as_array())
                                    .ok_or_else(|| {
                                        TestError::TypecheckError(
                                            "Headers field should be an array".to_string(),
                                        )
                                    })?;

                                // Headers should match row keys
                                assert_eq!(
                                    row_obj.len(),
                                    headers_arr.len(),
                                    "Number of headers should match number of row fields"
                                );

                                // Verify all headers are present in row
                                for header in headers_arr {
                                    assert!(header.is_string(), "Header should be a string");
                                    let header_str = header.as_str().unwrap();
                                    assert!(
                                        row_obj.contains_key(header_str),
                                        "Row should contain field '{}'",
                                        header_str
                                    );
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    assert!(
        affected_rows_found,
        "Should have found _affectedRows column in results"
    );

    Ok(())
}

#[tokio::test]
async fn test_update_affected_rows() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let update_query = r#"
        update UpdateUser($id: Int, $name: String) {
            user {
                @where { id = $id }
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

    // Check that we have result sets
    assert!(!rows.is_empty(), "Should have at least one result set");

    // Find _affectedRows result set
    let mut affected_rows_found = false;
    for mut rows_set in rows {
        let column_count = rows_set.column_count();
        for i in 0..column_count {
            if let Some(col_name) = rows_set.column_name(i) {
                if col_name == "_affectedRows" {
                    affected_rows_found = true;
                    if let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                        if let Ok(json_str) = row.get::<String>(i as i32) {
                            let json_value: serde_json::Value = serde_json::from_str(&json_str)
                                .map_err(|e| {
                                    TestError::TypecheckError(format!(
                                        "Failed to parse JSON: {}",
                                        e
                                    ))
                                })?;

                            assert!(json_value.is_array(), "_affectedRows should be an array");

                            let arr = json_value.as_array().unwrap();
                            // Should have at least one affected row
                            assert!(arr.len() > 0, "Should have at least one affected row");

                            // Verify structure
                            if let Some(affected_row) = arr.first() {
                                assert!(
                                    affected_row.is_object(),
                                    "Affected row should be an object"
                                );

                                let obj = affected_row.as_object().unwrap();
                                assert_eq!(
                                    obj["table_name"].as_str().unwrap(),
                                    "users",
                                    "Table name should be 'users'"
                                );
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    assert!(
        affected_rows_found,
        "Should have found _affectedRows column in results"
    );

    Ok(())
}

#[tokio::test]
async fn test_delete_affected_rows() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let delete_query = r#"
        delete RemoveUser($id: Int) {
            user {
                @where { id = $id }
                id
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("id".to_string(), libsql::Value::Integer(1));

    let rows = db.execute_query_with_params(delete_query, params).await?;

    // Check that we have result sets
    assert!(!rows.is_empty(), "Should have at least one result set");

    // Find _affectedRows result set
    let mut affected_rows_found = false;
    for mut rows_set in rows {
        let column_count = rows_set.column_count();
        for i in 0..column_count {
            if let Some(col_name) = rows_set.column_name(i) {
                if col_name == "_affectedRows" {
                    affected_rows_found = true;
                    if let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                        if let Ok(json_str) = row.get::<String>(i as i32) {
                            let json_value: serde_json::Value = serde_json::from_str(&json_str)
                                .map_err(|e| {
                                    TestError::TypecheckError(format!(
                                        "Failed to parse JSON: {}",
                                        e
                                    ))
                                })?;

                            assert!(json_value.is_array(), "_affectedRows should be an array");

                            let arr = json_value.as_array().unwrap();
                            // Should have at least one affected row
                            assert!(arr.len() > 0, "Should have at least one affected row");

                            // Verify structure
                            if let Some(affected_row) = arr.first() {
                                assert!(
                                    affected_row.is_object(),
                                    "Affected row should be an object"
                                );

                                let obj = affected_row.as_object().unwrap();
                                assert_eq!(
                                    obj["table_name"].as_str().unwrap(),
                                    "users",
                                    "Table name should be 'users'"
                                );
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    assert!(
        affected_rows_found,
        "Should have found _affectedRows column in results"
    );

    Ok(())
}

#[tokio::test]
async fn test_nested_insert_affected_rows() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    let insert_query = r#"
        insert CreateUserWithPost($name: String, $title: String) {
            user {
                name = $name
                status = Active
                posts {
                    title = $title
                    content = "Content"
                }
            }
        }
    "#;

    let mut params = HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    params.insert(
        "title".to_string(),
        libsql::Value::Text("First Post".to_string()),
    );

    let rows = db.execute_insert_with_params(insert_query, params).await?;

    // Check that we have result sets
    assert!(!rows.is_empty(), "Should have at least one result set");

    // Find _affectedRows result set
    let mut affected_rows_found = false;
    let mut table_names = Vec::new();

    for mut rows_set in rows {
        let column_count = rows_set.column_count();
        for i in 0..column_count {
            if let Some(col_name) = rows_set.column_name(i) {
                if col_name == "_affectedRows" {
                    affected_rows_found = true;
                    if let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                        if let Ok(json_str) = row.get::<String>(i as i32) {
                            let json_value: serde_json::Value = serde_json::from_str(&json_str)
                                .map_err(|e| {
                                    TestError::TypecheckError(format!(
                                        "Failed to parse JSON: {}",
                                        e
                                    ))
                                })?;

                            assert!(json_value.is_array(), "_affectedRows should be an array");

                            let arr = json_value.as_array().unwrap();

                            // Should have rows from both users and posts tables
                            assert!(
                                arr.len() >= 2,
                                "Should have affected rows from multiple tables, got {}",
                                arr.len()
                            );

                            // Collect table names
                            for affected_row in arr {
                                if let Some(obj) = affected_row.as_object() {
                                    if let Some(table_name) = obj["table_name"].as_str() {
                                        table_names.push(table_name.to_string());
                                    }
                                }
                            }

                            // Verify we have both tables
                            assert!(
                                table_names.contains(&"users".to_string()),
                                "Should have affected rows from 'users' table"
                            );
                            assert!(
                                table_names.contains(&"posts".to_string()),
                                "Should have affected rows from 'posts' table"
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    assert!(
        affected_rows_found,
        "Should have found _affectedRows column in results"
    );

    Ok(())
}
