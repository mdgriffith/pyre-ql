use crate::helpers::schema;
use crate::helpers::test_database::TestDatabase;
use crate::helpers::TestError;
use pyre::generate::sql::to_sql::SqlAndParams;

/// Test that multiple inserts to the same table in one mutation don't cause SQL collisions
/// This tests the scenario where a user has multiple posts nested in the same insert using aliases
#[tokio::test]
async fn test_multiple_posts_insert() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Insert a user with multiple posts in a single mutation using aliases
    // (The parser requires aliases when the same field name is used multiple times)
    let insert_query = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
                firstPost: posts {
                    title = "First Post"
                    content = "Content of first post"
                }
                secondPost: posts {
                    title = "Second Post"
                    content = "Content of second post"
                }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Active".to_string()),
    );

    // Generate SQL to check for collisions
    let sql_statements = db.generate_query_sql(insert_query)?;

    // Check that temp table names don't collide
    // If there are multiple posts inserts, they should have unique temp table names
    let temp_table_names: Vec<String> = sql_statements
        .iter()
        .filter_map(|(_, sql_stmt)| {
            if let SqlAndParams::Sql(sql) = sql_stmt {
                // Look for "create temp table" statements
                if sql.contains("create temp table") {
                    // Extract temp table name - look for "create temp table NAME" or "create temp table NAME as"
                    let sql_lower = sql.to_lowercase();
                    if let Some(start_idx) = sql_lower.find("create temp table") {
                        let after_create = &sql[start_idx + "create temp table".len()..];
                        // Skip whitespace
                        let after_whitespace = after_create.trim_start();
                        // Extract the table name (until whitespace, newline, or "as")
                        let name_end = after_whitespace
                            .find(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
                            .unwrap_or(after_whitespace.len());
                        let name = after_whitespace[..name_end].trim_end();
                        if !name.is_empty() {
                            return Some(name.to_string());
                        }
                    }
                }
            }
            None
        })
        .collect();

    // Verify no duplicate temp table names
    let mut seen = std::collections::HashSet::new();
    for name in &temp_table_names {
        assert!(
            !seen.contains(name),
            "Duplicate temp table name found: {}. This indicates a SQL collision. All temp tables: {:?}",
            name,
            temp_table_names
        );
        seen.insert(name);
    }

    // Execute the insert to verify it works
    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the user was created
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();
    assert_eq!(users.len(), 1, "Should have exactly 1 user");

    // Verify both posts were created by querying them
    let query_posts = r#"
        query GetPosts {
            post {
                id
                title
                content
            }
        }
    "#;

    let post_rows = db.execute_query(query_posts).await?;
    let post_results = db.parse_query_results(post_rows).await?;

    assert!(
        post_results.contains_key("post"),
        "Results should contain 'post' field"
    );
    let posts = post_results.get("post").unwrap();
    assert!(
        posts.len() >= 2,
        "Should have at least 2 posts, but got {}. Posts: {:#}",
        posts.len(),
        serde_json::json!(posts)
    );

    // Verify both posts have the expected titles
    let titles: Vec<String> = posts
        .iter()
        .filter_map(|p| {
            p.get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(
        titles.contains(&"First Post".to_string()),
        "Should contain 'First Post'. Found titles: {:?}",
        titles
    );
    assert!(
        titles.contains(&"Second Post".to_string()),
        "Should contain 'Second Post'. Found titles: {:?}",
        titles
    );

    Ok(())
}

/// Test multiple accounts inserted in the same mutation using aliases
#[tokio::test]
async fn test_multiple_accounts_insert() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Insert a user with multiple accounts in a single mutation using aliases
    // (The parser requires aliases when the same field name is used multiple times)
    let insert_query = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
                firstAccount: accounts {
                    name = "Account 1"
                    status = "active"
                }
                secondAccount: accounts {
                    name = "Account 2"
                    status = "inactive"
                }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Active".to_string()),
    );

    // Generate SQL to check for collisions
    let sql_statements = db.generate_query_sql(insert_query)?;

    // Check that temp table names don't collide
    let temp_table_names: Vec<String> = sql_statements
        .iter()
        .filter_map(|(_, sql_stmt)| {
            if let SqlAndParams::Sql(sql) = sql_stmt {
                if sql.contains("create temp table") {
                    let sql_lower = sql.to_lowercase();
                    if let Some(start_idx) = sql_lower.find("create temp table") {
                        let after_create = &sql[start_idx + "create temp table".len()..];
                        let after_whitespace = after_create.trim_start();
                        let name_end = after_whitespace
                            .find(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
                            .unwrap_or(after_whitespace.len());
                        let name = after_whitespace[..name_end].trim_end();
                        if !name.is_empty() {
                            return Some(name.to_string());
                        }
                    }
                }
            }
            None
        })
        .collect();

    // Verify no duplicate temp table names
    let mut seen = std::collections::HashSet::new();
    for name in &temp_table_names {
        assert!(
            !seen.contains(name),
            "Duplicate temp table name found: {}. This indicates a SQL collision. All temp tables: {:?}",
            name,
            temp_table_names
        );
        seen.insert(name);
    }

    // Execute the insert to verify it works
    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the user was created
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );

    // Verify both accounts were created
    let query_accounts = r#"
        query GetAccounts {
            account {
                id
                name
                status
            }
        }
    "#;

    let account_rows = db.execute_query(query_accounts).await?;
    let account_results = db.parse_query_results(account_rows).await?;

    assert!(
        account_results.contains_key("account"),
        "Results should contain 'account' field"
    );
    let accounts = account_results.get("account").unwrap();
    assert!(
        accounts.len() >= 2,
        "Should have at least 2 accounts, but got {}. Accounts: {:#}",
        accounts.len(),
        serde_json::json!(accounts)
    );

    // Verify both accounts have the expected names
    let names: Vec<String> = accounts
        .iter()
        .filter_map(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(
        names.contains(&"Account 1".to_string()),
        "Should contain 'Account 1'. Found names: {:?}",
        names
    );
    assert!(
        names.contains(&"Account 2".to_string()),
        "Should contain 'Account 2'. Found names: {:?}",
        names
    );

    Ok(())
}

/// Test multiple nested inserts to the same table with different field names (using aliases)
/// This tests that aliases help avoid collisions
#[tokio::test]
async fn test_multiple_posts_with_aliases() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Insert a user with multiple posts using aliases
    let insert_query = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
                firstPost: posts {
                    title = "First Post"
                    content = "Content of first post"
                }
                secondPost: posts {
                    title = "Second Post"
                    content = "Content of second post"
                }
            }
        }
    "#;

    let mut params = std::collections::HashMap::new();
    params.insert(
        "name".to_string(),
        libsql::Value::Text("Charlie".to_string()),
    );
    params.insert(
        "status".to_string(),
        libsql::Value::Text("Active".to_string()),
    );

    // Generate SQL to check for collisions
    let sql_statements = db.generate_query_sql(insert_query)?;

    // Check that temp table names are unique (should use aliases)
    let temp_table_names: Vec<String> = sql_statements
        .iter()
        .filter_map(|(_, sql_stmt)| {
            if let SqlAndParams::Sql(sql) = sql_stmt {
                if sql.contains("create temp table") {
                    let sql_lower = sql.to_lowercase();
                    if let Some(start_idx) = sql_lower.find("create temp table") {
                        let after_create = &sql[start_idx + "create temp table".len()..];
                        let after_whitespace = after_create.trim_start();
                        let name_end = after_whitespace
                            .find(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
                            .unwrap_or(after_whitespace.len());
                        let name = after_whitespace[..name_end].trim_end();
                        if !name.is_empty() {
                            return Some(name.to_string());
                        }
                    }
                }
            }
            None
        })
        .collect();

    // Verify no duplicate temp table names
    let mut seen = std::collections::HashSet::new();
    for name in &temp_table_names {
        assert!(
            !seen.contains(name),
            "Duplicate temp table name found: {}. This indicates a SQL collision. All temp tables: {:?}",
            name,
            temp_table_names
        );
        seen.insert(name);
    }

    // If aliases are used, temp table names should reflect them
    // The important thing is that they're unique (which is already checked above)
    // Note: The exact naming scheme depends on implementation

    // Execute the insert to verify it works
    let rows = db.execute_insert_with_params(insert_query, params).await?;
    let results = db.parse_query_results(rows).await?;

    // Verify the user was created
    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );

    // Verify both posts were created
    let query_posts = r#"
        query GetPosts {
            post {
                id
                title
                content
            }
        }
    "#;

    let post_rows = db.execute_query(query_posts).await?;
    let post_results = db.parse_query_results(post_rows).await?;

    assert!(
        post_results.contains_key("post"),
        "Results should contain 'post' field"
    );
    let posts = post_results.get("post").unwrap();
    assert!(
        posts.len() >= 2,
        "Should have at least 2 posts, but got {}. Posts: {:#}",
        posts.len(),
        serde_json::json!(posts)
    );

    Ok(())
}

/// Test that INSERT statements themselves don't collide when inserting multiple rows to the same table
/// This checks that the SQL generation creates proper INSERT statements
#[tokio::test]
async fn test_multiple_posts_insert_sql_structure() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;

    // Use aliases since duplicate field names aren't allowed
    let insert_query = r#"
        insert CreateUser($name: String, $status: Status) {
            user {
                name = $name
                status = $status
                firstPost: posts {
                    title = "First Post"
                    content = "Content of first post"
                }
                secondPost: posts {
                    title = "Second Post"
                    content = "Content of second post"
                }
            }
        }
    "#;

    // Generate SQL to inspect the structure
    let sql_statements = db.generate_query_sql(insert_query)?;

    // Count INSERT statements for the posts table
    let mut posts_insert_count = 0;
    for (_, sql_stmt) in &sql_statements {
        if let SqlAndParams::Sql(sql) = sql_stmt {
            // Look for INSERT INTO posts statements
            if sql.to_lowercase().contains("insert into") && sql.to_lowercase().contains("posts") {
                posts_insert_count += 1;
            }
        }
    }

    // We should have at least one INSERT statement for posts
    // The exact number depends on implementation - could be one batched insert or multiple
    assert!(
        posts_insert_count > 0,
        "Should have at least one INSERT statement for posts table. Found {} INSERT statements.",
        posts_insert_count
    );

    // Verify that all INSERT statements are syntactically valid
    // by checking they don't have obvious syntax errors
    for (_, sql_stmt) in &sql_statements {
        if let SqlAndParams::Sql(sql) = sql_stmt {
            // Check for common SQL syntax issues
            assert!(
                !sql.contains("insert into insert into"),
                "Found duplicate 'insert into' keywords: {}",
                sql
            );
            assert!(
                !sql.contains("from from"),
                "Found duplicate 'from' keywords: {}",
                sql
            );
            assert!(
                !sql.contains("select select"),
                "Found duplicate 'select' keywords: {}",
                sql
            );
        }
    }

    Ok(())
}
