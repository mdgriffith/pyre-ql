#[path = "helpers/mod.rs"]
mod helpers;

use helpers::schema;
use helpers::test_database::TestDatabase;
use helpers::TestError;
use pyre::seed;

#[tokio::test]
async fn test_seed_generates_valid_sql() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Generate seed data
    let operations = seed::seed_database(&db.schema, &db.context, None);

    // Verify we got some operations
    assert!(
        !operations.is_empty(),
        "Should generate at least some SQL operations"
    );

    // Execute all INSERT statements
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        // Verify SQL is valid by attempting to execute it
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_data_can_be_queried() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Generate and execute seed data
    let operations = seed::seed_database(&db.schema, &db.context, None);
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Query all tables to verify data exists
    let user_query = r#"
        query GetUsers {
            user {
                id
                name
                status
            }
        }
    "#;

    let rows = db.execute_query(user_query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    assert!(
        !users.is_empty(),
        "Should have at least some users from seed data"
    );

    // Verify user fields match schema
    for user in users {
        assert!(user.get("id").is_some(), "User should have id field");
        assert!(user.get("name").is_some(), "User should have name field");
        assert!(
            user.get("status").is_some(),
            "User should have status field"
        );
    }

    // Query posts
    let post_query = r#"
        query GetPosts {
            post {
                id
                title
                content
                authorId
            }
        }
    "#;

    let rows = db.execute_query(post_query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("post"),
        "Results should contain 'post' field"
    );
    let posts = results.get("post").unwrap();
    assert!(
        !posts.is_empty(),
        "Should have at least some posts from seed data"
    );

    // Verify post fields match schema
    for post in posts {
        assert!(post.get("id").is_some(), "Post should have id field");
        assert!(post.get("title").is_some(), "Post should have title field");
        assert!(
            post.get("content").is_some(),
            "Post should have content field"
        );
        assert!(
            post.get("authorId").is_some(),
            "Post should have authorId field"
        );
    }

    // Query accounts
    let account_query = r#"
        query GetAccounts {
            account {
                id
                userId
                name
                status
            }
        }
    "#;

    let rows = db.execute_query(account_query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("account"),
        "Results should contain 'account' field"
    );
    let accounts = results.get("account").unwrap();
    assert!(
        !accounts.is_empty(),
        "Should have at least some accounts from seed data"
    );

    // Verify account fields match schema
    for account in accounts {
        assert!(account.get("id").is_some(), "Account should have id field");
        assert!(
            account.get("userId").is_some(),
            "Account should have userId field"
        );
        assert!(
            account.get("name").is_some(),
            "Account should have name field"
        );
        assert!(
            account.get("status").is_some(),
            "Account should have status field"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_with_custom_options() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Create custom options with specific row counts
    let mut options = seed::Options::default();
    options.default_rows_per_table = 10;
    options.table_rows.insert("users".to_string(), 5);
    options.table_rows.insert("posts".to_string(), 20);

    // Generate seed data with custom options
    let operations = seed::seed_database(&db.schema, &db.context, Some(options));

    // Execute seed data
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Verify row counts match expectations (approximately)
    let mut user_rows = db.execute_raw("SELECT COUNT(*) FROM users").await?;
    let mut user_count = 0;
    while let Some(row) = user_rows.next().await.map_err(TestError::Database)? {
        user_count = row.get::<i64>(0).map_err(TestError::Database)?;
    }
    assert_eq!(
        user_count, 5,
        "Should have exactly 5 users as specified in options"
    );

    let mut post_rows = db.execute_raw("SELECT COUNT(*) FROM posts").await?;
    let mut post_count = 0;
    while let Some(row) = post_rows.next().await.map_err(TestError::Database)? {
        post_count = row.get::<i64>(0).map_err(TestError::Database)?;
    }
    assert_eq!(
        post_count, 20,
        "Should have exactly 20 posts as specified in options"
    );

    Ok(())
}

#[tokio::test]
async fn test_seed_foreign_key_relationships() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Generate seed data
    let operations = seed::seed_database(&db.schema, &db.context, None);

    // Execute seed data
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Query posts with author relationship
    let query = r#"
        query GetPostsWithAuthors {
            post {
                id
                title
                authorId
                author {
                    id
                    name
                }
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("post"),
        "Results should contain 'post' field"
    );
    let posts = results.get("post").unwrap();
    assert!(!posts.is_empty(), "Should have posts");

    // Verify foreign key relationships are valid
    for post in posts {
        let author_id = post
            .get("authorId")
            .and_then(|v| v.as_i64())
            .expect("Post should have authorId");

        // Check if author relationship exists (it might be null if the user was deleted or doesn't exist)
        if let Some(author) = post.get("author").and_then(|v| v.as_object()) {
            let author_id_from_rel = author
                .get("id")
                .and_then(|v| v.as_i64())
                .expect("Author should have id");

            assert_eq!(
                author_id, author_id_from_rel,
                "Post authorId should match author.id from relationship"
            );
        } else {
            // If author relationship is missing, at least verify the authorId exists in users table
            let mut user_check = db
                .execute_raw(&format!(
                    "SELECT COUNT(*) FROM users WHERE id = {}",
                    author_id
                ))
                .await?;
            let mut user_exists = 0;
            while let Some(row) = user_check.next().await.map_err(TestError::Database)? {
                user_exists = row.get::<i64>(0).map_err(TestError::Database)?;
            }
            assert!(
                user_exists > 0,
                "Post authorId {} should reference an existing user",
                author_id
            );
        }
    }

    // Query accounts with user relationship
    let account_query = r#"
        query GetAccountsWithUsers {
            account {
                id
                userId
                user {
                    id
                    name
                }
            }
        }
    "#;

    let rows = db.execute_query(account_query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("account"),
        "Results should contain 'account' field"
    );
    let accounts = results.get("account").unwrap();

    // Verify foreign key relationships are valid
    for account in accounts {
        let user_id = account
            .get("userId")
            .and_then(|v| v.as_i64())
            .expect("Account should have userId");
        let user = account
            .get("user")
            .and_then(|v| v.as_object())
            .expect("Account should have user relationship");
        let user_id_from_rel = user
            .get("id")
            .and_then(|v| v.as_i64())
            .expect("User should have id");

        assert_eq!(
            user_id, user_id_from_rel,
            "Account userId should match user.id from relationship"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_with_foreign_key_ratios() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Create options with custom foreign key ratio
    let mut options = seed::Options::default();
    options.default_rows_per_table = 10;
    // Set ratio: 10 posts per user
    options
        .foreign_key_ratios
        .insert(("User".to_string(), "Post".to_string()), 10.0);

    // Generate seed data
    let operations = seed::seed_database(&db.schema, &db.context, Some(options));

    // Execute seed data
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Verify approximate ratio (allowing for ±20% variation)
    let mut user_rows = db.execute_raw("SELECT COUNT(*) FROM users").await?;
    let mut user_count = 0;
    while let Some(row) = user_rows.next().await.map_err(TestError::Database)? {
        user_count = row.get::<i64>(0).map_err(TestError::Database)?;
    }

    let mut post_rows = db.execute_raw("SELECT COUNT(*) FROM posts").await?;
    let mut post_count = 0;
    while let Some(row) = post_rows.next().await.map_err(TestError::Database)? {
        post_count = row.get::<i64>(0).map_err(TestError::Database)?;
    }

    if user_count > 0 {
        let actual_ratio = post_count as f64 / user_count as f64;
        // Should be approximately 10, but allow for variation
        assert!(
            actual_ratio >= 8.0 && actual_ratio <= 12.0,
            "Post-to-user ratio should be approximately 10:1, but got {}:1 ({} posts / {} users)",
            actual_ratio,
            post_count,
            user_count
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_union_types() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Generate seed data
    let operations = seed::seed_database(&db.schema, &db.context, None);

    // Execute seed data
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Query users with status (union type)
    let query = r#"
        query GetUsersWithStatus {
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
    assert!(!users.is_empty(), "Should have users");

    // Verify status field exists and is valid
    for user in users {
        let status = user.get("status").expect("User should have status field");
        // Status should be a string (variant name) or an object (variant with fields)
        assert!(
            status.is_string() || status.is_object(),
            "Status should be a string or object, got: {:?}",
            status
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_all_tables_have_data() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Generate seed data
    let operations = seed::seed_database(&db.schema, &db.context, None);

    // Execute seed data
    let conn = db.db.connect().map_err(TestError::Database)?;
    for op in &operations {
        conn.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Verify all tables in the schema have data
    for (table_name, _table) in &db.context.tables {
        let sql_table_name = pyre::ast::get_tablename(&_table.record.name, &_table.record.fields);
        let count_query = format!(
            "SELECT COUNT(*) FROM {}",
            pyre::ext::string::quote(&sql_table_name)
        );

        let mut rows = db.execute_raw(&count_query).await?;
        let mut count = 0;
        while let Some(row) = rows.next().await.map_err(TestError::Database)? {
            count = row.get::<i64>(0).map_err(TestError::Database)?;
        }

        assert!(
            count > 0,
            "Table {} (record {}) should have at least one row, but got {}",
            sql_table_name,
            table_name,
            count
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_seed_deterministic() -> Result<(), TestError> {
    let db1 = TestDatabase::new(&schema::full_schema()).await?;
    let db2 = TestDatabase::new(&schema::full_schema()).await?;

    // Create options with the same seed
    let mut options = seed::Options::default();
    options.seed = Some(42);
    options.default_rows_per_table = 10;

    // Generate seed data for both databases with the same seed
    let operations1 = seed::seed_database(&db1.schema, &db1.context, Some(options.clone()));
    let operations2 = seed::seed_database(&db2.schema, &db2.context, Some(options));

    // Verify that the SQL operations are identical (deterministic)
    assert_eq!(
        operations1.len(),
        operations2.len(),
        "Should generate the same number of operations"
    );

    for (i, (op1, op2)) in operations1.iter().zip(operations2.iter()).enumerate() {
        assert_eq!(
            op1.sql, op2.sql,
            "Operation {} should be identical with the same seed",
            i
        );
    }

    // Execute on both databases
    let conn1 = db1.db.connect().map_err(TestError::Database)?;
    for op in &operations1 {
        conn1.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    let conn2 = db2.db.connect().map_err(TestError::Database)?;
    for op in &operations2 {
        conn2.execute(&op.sql, ()).await.map_err(|e| {
            TestError::TypecheckError(format!(
                "Failed to execute seed SQL: {}\nSQL: {}",
                e, op.sql
            ))
        })?;
    }

    // Verify data is the same in both databases
    let mut rows1 = db1.execute_raw("SELECT COUNT(*) FROM users").await?;
    let mut count1 = 0;
    while let Some(row) = rows1.next().await.map_err(TestError::Database)? {
        count1 = row.get::<i64>(0).map_err(TestError::Database)?;
    }

    let mut rows2 = db2.execute_raw("SELECT COUNT(*) FROM users").await?;
    let mut count2 = 0;
    while let Some(row) = rows2.next().await.map_err(TestError::Database)? {
        count2 = row.get::<i64>(0).map_err(TestError::Database)?;
    }

    assert_eq!(
        count1, count2,
        "Both databases should have the same number of users"
    );

    Ok(())
}
