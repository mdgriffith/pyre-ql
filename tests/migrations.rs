#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{schema, TestDatabase, TestError};
use pyre::db::diff;
use pyre::db::introspect;
use pyre::parser;
use pyre::typecheck;

#[tokio::test]
async fn test_migration_from_v1_to_v2() -> Result<(), TestError> {
    // Start with schema v1
    let db = TestDatabase::new(schema::SCHEMA_V1).await?;

    // Introspect the database to get actual tables
    let conn = db.db.connect().map_err(TestError::Database)?;
    let mut rows = conn
        .query(introspect::INTROSPECT_UNINITIALIZED_SQL, ())
        .await
        .map_err(TestError::Database)?;

    let mut introspection_raw = None;
    while let Some(row) = rows.next().await.map_err(TestError::Database)? {
        let result: String = row.get(0).map_err(TestError::Database)?;
        introspection_raw = Some(
            serde_json::from_str::<introspect::IntrospectionRaw>(&result).map_err(|e| {
                TestError::TypecheckError(format!("Failed to parse introspection: {}", e))
            })?,
        );
    }

    let introspection_raw = introspection_raw.ok_or(TestError::TypecheckError(
        "Failed to get introspection result".to_string(),
    ))?;

    // Recreate the context from the schema since Context doesn't implement Clone
    let database = pyre::ast::Database {
        schemas: vec![db.schema.clone()],
    };
    let current_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    // Build introspection with actual tables from database
    let introspection = introspect::Introspection {
        tables: introspection_raw.tables,
        migration_state: introspect::MigrationState::NoMigrationTable,
        schema: introspect::SchemaResult::Success {
            schema: db.schema.clone(),
            context: current_context,
        },
    };

    // Parse schema v2
    let mut new_schema = pyre::ast::Schema::default();
    parser::run("schema.pyre", schema::SCHEMA_V2, &mut new_schema)
        .map_err(|e| TestError::ParseError(pyre::parser::render_error(schema::SCHEMA_V2, e)))?;

    // Typecheck new schema
    let database = pyre::ast::Database {
        schemas: vec![new_schema.clone()],
    };
    let new_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    // Calculate diff - use new_context to create tables from new schema
    let db_diff = diff::diff(&new_context, &new_schema, &introspection);

    // Log the diff structure
    eprintln!("=== DB DIFF ===");
    eprintln!(
        "Added tables: {:?}",
        db_diff.added.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    eprintln!(
        "Removed tables: {:?}",
        db_diff.removed.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    eprintln!(
        "Modified records: {:?}",
        db_diff
            .modified_records
            .iter()
            .map(|r| &r.name)
            .collect::<Vec<_>>()
    );
    for record_diff in &db_diff.modified_records {
        eprintln!(
            "  {} changes: {:?}",
            record_diff.name,
            record_diff.changes.len()
        );
        for change in &record_diff.changes {
            eprintln!("    {:?}", change);
        }
    }

    // Generate and log SQL
    let migration_sql = diff::to_sql::to_sql(&db_diff);
    eprintln!("\n=== MIGRATION SQL ===");
    for sql_stmt in &migration_sql {
        match sql_stmt {
            pyre::generate::sql::to_sql::SqlAndParams::Sql(sql) => {
                eprintln!("{}", sql);
            }
            pyre::generate::sql::to_sql::SqlAndParams::SqlWithParams { sql, args } => {
                eprintln!("{} (with params: {:?})", sql, args);
            }
        }
    }

    // Log what's in the introspection
    eprintln!("\n=== INTROSPECTION ===");
    eprintln!(
        "Introspection tables: {:?}",
        introspection
            .tables
            .iter()
            .map(|t| &t.name)
            .collect::<Vec<_>>()
    );
    for table in &introspection.tables {
        eprintln!("  Table {}:", table.name);
        for col in &table.columns {
            eprintln!("    Column: {} ({})", col.name, col.column_type);
        }
    }

    // Log what's actually in the database
    eprintln!("\n=== DATABASE CONTENTS ===");
    let conn = db.db.connect().map_err(TestError::Database)?;
    let mut rows = conn.query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_pyre_%'", ()).await.map_err(TestError::Database)?;
    let mut table_names = Vec::new();
    while let Some(row) = rows.next().await.map_err(TestError::Database)? {
        let name: String = row.get(0).map_err(TestError::Database)?;
        table_names.push(name);
    }
    eprintln!("Tables in database: {:?}", table_names);
    for table_name in &table_names {
        let mut col_rows = conn
            .query(&format!("PRAGMA table_info(\"{}\")", table_name), ())
            .await
            .map_err(TestError::Database)?;
        eprintln!("  Table {} columns:", table_name);
        while let Some(col_row) = col_rows.next().await.map_err(TestError::Database)? {
            let _cid: i32 = col_row.get(0).map_err(TestError::Database)?;
            let name: String = col_row.get(1).map_err(TestError::Database)?;
            let col_type: String = col_row.get(2).map_err(TestError::Database)?;
            let notnull: i32 = col_row.get(3).map_err(TestError::Database)?;
            let pk: i32 = col_row.get(5).map_err(TestError::Database)?;
            eprintln!(
                "    {}: {} (notnull: {}, pk: {})",
                name, col_type, notnull, pk
            );
        }
    }

    // Verify that the diff includes adding the Post table (table name is pluralized: "posts")
    let has_post_table = db_diff.added.iter().any(|t| t.name == "posts");
    assert!(
        has_post_table,
        "Migration should add Post table (as 'posts')"
    );

    // Verify that User table has modifications (new posts relationship) (table name is pluralized: "users")
    let user_table_diff = db_diff.modified_records.iter().find(|r| r.name == "users");
    assert!(
        user_table_diff.is_some(),
        "Migration should modify User table (as 'users')"
    );

    Ok(())
}

#[tokio::test]
async fn test_migration_from_v2_to_v3() -> Result<(), TestError> {
    // Start with schema v2
    let db = TestDatabase::new(schema::SCHEMA_V2).await?;

    // Introspect the database to get actual tables
    let conn = db.db.connect().map_err(TestError::Database)?;
    let mut rows = conn
        .query(introspect::INTROSPECT_UNINITIALIZED_SQL, ())
        .await
        .map_err(TestError::Database)?;

    let mut introspection_raw = None;
    while let Some(row) = rows.next().await.map_err(TestError::Database)? {
        let result: String = row.get(0).map_err(TestError::Database)?;
        introspection_raw = Some(
            serde_json::from_str::<introspect::IntrospectionRaw>(&result).map_err(|e| {
                TestError::TypecheckError(format!("Failed to parse introspection: {}", e))
            })?,
        );
    }

    let introspection_raw = introspection_raw.ok_or(TestError::TypecheckError(
        "Failed to get introspection result".to_string(),
    ))?;

    // Recreate the context from the schema since Context doesn't implement Clone
    let database = pyre::ast::Database {
        schemas: vec![db.schema.clone()],
    };
    let current_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    // Build introspection with actual tables from database
    let introspection = introspect::Introspection {
        tables: introspection_raw.tables,
        migration_state: introspect::MigrationState::NoMigrationTable,
        schema: introspect::SchemaResult::Success {
            schema: db.schema.clone(),
            context: current_context,
        },
    };

    // Parse schema v3
    let mut new_schema = pyre::ast::Schema::default();
    parser::run("schema.pyre", schema::SCHEMA_V3, &mut new_schema)
        .map_err(|e| TestError::ParseError(pyre::parser::render_error(schema::SCHEMA_V3, e)))?;

    // Typecheck new schema
    let database = pyre::ast::Database {
        schemas: vec![new_schema.clone()],
    };
    let new_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    // Calculate diff - use new_context to create tables from new schema
    let db_diff = diff::diff(&new_context, &new_schema, &introspection);

    // Verify that the diff includes adding the Account table (table name is pluralized: "accounts")
    let has_account_table = db_diff.added.iter().any(|t| t.name == "accounts");
    assert!(
        has_account_table,
        "Migration should add Account table (as 'accounts')"
    );

    // Verify that User table has modifications (new accounts relationship) (table name is pluralized: "users")
    let user_table_diff = db_diff.modified_records.iter().find(|r| r.name == "users");
    assert!(
        user_table_diff.is_some(),
        "Migration should modify User table (as 'users')"
    );

    Ok(())
}

#[tokio::test]
async fn test_migration_from_v1_to_v3() -> Result<(), TestError> {
    // Start with schema v1
    let db = TestDatabase::new(schema::SCHEMA_V1).await?;

    // Create introspection from current database state
    // Recreate the context from the schema since Context doesn't implement Clone
    let database = pyre::ast::Database {
        schemas: vec![db.schema.clone()],
    };
    let current_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    let introspection = introspect::Introspection {
        tables: vec![],
        migration_state: introspect::MigrationState::NoMigrationTable,
        schema: introspect::SchemaResult::Success {
            schema: db.schema.clone(),
            context: current_context,
        },
    };

    // Parse schema v3
    let mut new_schema = pyre::ast::Schema::default();
    parser::run("schema.pyre", schema::SCHEMA_V3, &mut new_schema)
        .map_err(|e| TestError::ParseError(pyre::parser::render_error(schema::SCHEMA_V3, e)))?;

    // Typecheck new schema
    let database = pyre::ast::Database {
        schemas: vec![new_schema.clone()],
    };
    let new_context = typecheck::check_schema(&database)
        .map_err(|errors| TestError::TypecheckError(format!("Typecheck errors: {:?}", errors)))?;

    // Calculate diff - use new_context to create tables from new schema
    let db_diff = diff::diff(&new_context, &new_schema, &introspection);

    // Verify that the diff includes adding both Post and Account tables (table names are pluralized)
    let has_post_table = db_diff.added.iter().any(|t| t.name == "posts");
    let has_account_table = db_diff.added.iter().any(|t| t.name == "accounts");

    assert!(
        has_post_table,
        "Migration should add Post table (as 'posts')"
    );
    assert!(
        has_account_table,
        "Migration should add Account table (as 'accounts')"
    );

    Ok(())
}
