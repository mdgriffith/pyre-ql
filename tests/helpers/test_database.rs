use libsql::Database;
use pyre::ast;
use pyre::db::diff;
use pyre::db::introspect;
use pyre::db::migrate;
use pyre::error;
use pyre::parser;
use pyre::typecheck;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tempfile::TempDir;

use super::TestError;
use pyre::generate::sql::to_sql::SqlAndParams;

pub struct TestDatabase {
    pub db: Database,
    pub temp_dir: TempDir,
    pub context: typecheck::Context,
    pub schema: ast::Schema,
}

impl TestDatabase {
    /// Create a new test database with a schema
    pub async fn new(schema_source: &str) -> Result<Self, TestError> {
        let temp_dir = TempDir::new().map_err(TestError::Io)?;
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().ok_or(TestError::InvalidPath)?;

        // Create database
        let db = libsql::Builder::new_local(db_path_str)
            .build()
            .await
            .map_err(TestError::Database)?;

        // Parse schema
        let mut schema = ast::Schema::default();
        parser::run("schema.pyre", schema_source, &mut schema)
            .map_err(|e| TestError::ParseError(parser::render_error(schema_source, e)))?;

        // Typecheck schema
        let database = ast::Database {
            schemas: vec![schema.clone()],
        };
        let context = typecheck::check_schema(&database)
            .map_err(|errors| TestError::TypecheckError(format_errors(schema_source, &errors)))?;

        // Create empty introspection for a fresh database
        let introspection = introspect::Introspection {
            tables: vec![],
            migration_state: introspect::MigrationState::NoMigrationTable,
            schema: introspect::SchemaResult::Success {
                schema: ast::Schema::default(),
                context: typecheck::empty_context(),
            },
        };

        // Generate migration SQL
        let db_diff = diff::diff(&context, &schema, &introspection);
        let mut migration_sql = diff::to_sql::to_sql(&db_diff);

        // Add migration table creation if needed
        match introspection.migration_state {
            introspect::MigrationState::NoMigrationTable => {
                migration_sql.insert(
                    0,
                    SqlAndParams::Sql(migrate::CREATE_MIGRATION_TABLE.to_string()),
                );
                migration_sql.insert(
                    1,
                    SqlAndParams::Sql(migrate::CREATE_SCHEMA_TABLE.to_string()),
                );
            }
            introspect::MigrationState::MigrationTable { .. } => {}
        }

        // Add schema insertion
        let schema_string = pyre::generate::to_string::schema_to_string("", &schema);
        migration_sql.push(SqlAndParams::SqlWithParams {
            sql: migrate::INSERT_SCHEMA.to_string(),
            args: vec![schema_string],
        });

        // Execute migration
        let conn = db.connect().map_err(TestError::Database)?;
        let tx = conn
            .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
            .await
            .map_err(TestError::Database)?;

        // Collect SQL strings for error reporting
        let mut sql_strings: Vec<String> = Vec::new();
        for stmt in &migration_sql {
            match stmt {
                SqlAndParams::Sql(s) => {
                    sql_strings.push(s.clone());
                }
                SqlAndParams::SqlWithParams { sql: s, args } => {
                    sql_strings.push(format!("{} with args: {:?}", s, args));
                }
            }
        }

        for (idx, sql_stmt) in migration_sql.iter().enumerate() {
            match sql_stmt {
                SqlAndParams::Sql(sql) => {
                    if let Err(e) = tx.execute_batch(sql).await {
                        // On error, print all SQL statements for debugging
                        eprintln!(
                            "Migration failed at statement {} of {}. SQL statements:",
                            idx + 1,
                            sql_strings.len()
                        );
                        for (i, sql_str) in sql_strings.iter().enumerate() {
                            if i == idx {
                                eprintln!("  [{}] {} <-- FAILED HERE", i + 1, sql_str);
                            } else {
                                eprintln!("  [{}] {}", i + 1, sql_str);
                            }
                        }
                        return Err(TestError::Database(e));
                    }
                }
                SqlAndParams::SqlWithParams { sql, args } => {
                    // Convert args to libsql::Value
                    let values: Vec<libsql::Value> = args
                        .iter()
                        .map(|s| libsql::Value::Text(s.clone()))
                        .collect();
                    if let Err(e) = tx.execute(sql, libsql::params_from_iter(values)).await {
                        // On error, print all SQL statements for debugging
                        eprintln!(
                            "Migration failed at statement {} of {}. SQL statements:",
                            idx + 1,
                            sql_strings.len()
                        );
                        for (i, sql_str) in sql_strings.iter().enumerate() {
                            if i == idx {
                                eprintln!("  [{}] {} <-- FAILED HERE", i + 1, sql_str);
                            } else {
                                eprintln!("  [{}] {}", i + 1, sql_str);
                            }
                        }
                        return Err(TestError::Database(e));
                    }
                }
            }
        }

        tx.commit().await.map_err(TestError::Database)?;

        Ok(TestDatabase {
            db,
            temp_dir,
            context,
            schema,
        })
    }

    /// Execute a query and return the SQL that would be generated
    /// Returns a vector of (include_flag, sql) tuples where include_flag indicates if the statement returns results
    pub fn generate_query_sql(
        &self,
        query_source: &str,
    ) -> Result<Vec<(bool, SqlAndParams)>, TestError> {
        let query_list = parser::parse_query("query.pyre", query_source)
            .map_err(|e| TestError::ParseError(parser::render_error(query_source, e)))?;

        // Use the existing context, but update the filepath
        // Note: We can't clone Context because Table and Type don't implement Clone
        // So we'll use a reference to the existing context
        let context = &self.context;

        let query_info = typecheck::check_queries(&query_list, &context)
            .map_err(|errors| TestError::TypecheckError(format_errors(query_source, &errors)))?;

        // Get the first query
        let query = query_list
            .queries
            .iter()
            .find_map(|q| match q {
                ast::QueryDef::Query(q) => Some(q),
                _ => None,
            })
            .ok_or(TestError::NoQueryFound)?;

        let info = query_info
            .get(&query.name)
            .ok_or(TestError::NoQueryInfoFound)?;

        // Get the table for this query
        let table_field = query
            .fields
            .iter()
            .find_map(|f| match f {
                ast::TopLevelQueryField::Field(f) => Some(f),
                _ => None,
            })
            .ok_or(TestError::NoQueryFound)?;

        let table = context
            .tables
            .get(&table_field.name)
            .ok_or(TestError::NoQueryFound)?;

        // Generate SQL using the to_string function
        let prepared_statements =
            pyre::generate::sql::to_string(context, query, info, table, table_field);

        // Convert Prepared statements to SqlAndParams
        // Note: For inserts and complex selects, we need to include ALL statements
        // (even those marked as ignore) because they create temp tables that are needed
        // We preserve the include flag to know which statements return results
        let mut sql_statements = Vec::new();
        for prepared in prepared_statements {
            sql_statements.push((prepared.include, SqlAndParams::Sql(prepared.sql)));
        }

        Ok(sql_statements)
    }

    /// Execute a query with parameters and return results
    pub async fn execute_query_with_params(
        &self,
        query_source: &str,
        params: HashMap<String, libsql::Value>,
    ) -> Result<Vec<libsql::Rows>, TestError> {
        // Parse query to get parameter names in order
        let query_list = parser::parse_query("query.pyre", query_source)
            .map_err(|e| TestError::ParseError(parser::render_error(query_source, e)))?;

        let query = query_list
            .queries
            .iter()
            .find_map(|q| match q {
                ast::QueryDef::Query(q) => Some(q),
                _ => None,
            })
            .ok_or(TestError::NoQueryFound)?;

        // Extract parameter names in order
        let param_names: Vec<String> = query.args.iter().map(|arg| arg.name.clone()).collect();

        // Build parameter values in order
        let param_values: Vec<libsql::Value> = param_names
            .iter()
            .map(|name| params.get(name).cloned().unwrap_or(libsql::Value::Null))
            .collect();

        let sql_statements = self.generate_query_sql(query_source)?;

        let conn = self.db.connect().map_err(TestError::Database)?;
        let mut results = Vec::new();

        // Execute statements sequentially, using include flag to determine if they return results
        for (include, sql_stmt) in sql_statements {
            match sql_stmt {
                SqlAndParams::Sql(sql) => {
                    let sql_with_params = if param_names.is_empty() {
                        sql.clone()
                    } else {
                        replace_params_positional(&sql, &param_names)
                    };

                    if include {
                        // This statement returns results - use query()
                        if param_values.is_empty() {
                            let rows = conn
                                .query(&sql_with_params, ())
                                .await
                                .map_err(TestError::Database)?;
                            results.push(rows);
                        } else {
                            let rows = conn
                                .query(
                                    &sql_with_params,
                                    libsql::params_from_iter(param_values.clone()),
                                )
                                .await
                                .map_err(TestError::Database)?;
                            results.push(rows);
                        }
                    } else {
                        // This statement doesn't return results - use execute()
                        if param_values.is_empty() {
                            conn.execute(&sql_with_params, ())
                                .await
                                .map_err(TestError::Database)?;
                        } else {
                            conn.execute(
                                &sql_with_params,
                                libsql::params_from_iter(param_values.clone()),
                            )
                            .await
                            .map_err(TestError::Database)?;
                        }
                    }
                }
                SqlAndParams::SqlWithParams { sql, args } => {
                    let mut values: Vec<libsql::Value> =
                        args.into_iter().map(|s| libsql::Value::Text(s)).collect();
                    values.extend(param_values.clone());
                    let sql_with_params = if param_names.is_empty() {
                        sql.clone()
                    } else {
                        replace_params_positional(&sql, &param_names)
                    };

                    if include {
                        let rows = conn
                            .query(&sql_with_params, libsql::params_from_iter(values))
                            .await
                            .map_err(TestError::Database)?;
                        results.push(rows);
                    } else {
                        conn.execute(&sql_with_params, libsql::params_from_iter(values))
                            .await
                            .map_err(TestError::Database)?;
                    }
                }
            }
        }

        Ok(results)
    }

    /// Execute a query and return results
    pub async fn execute_query(&self, query_source: &str) -> Result<Vec<libsql::Rows>, TestError> {
        self.execute_query_with_params(query_source, HashMap::new())
            .await
    }

    /// Execute an insert query with parameters
    pub async fn execute_insert_with_params(
        &self,
        insert_query: &str,
        params: HashMap<String, libsql::Value>,
    ) -> Result<Vec<libsql::Rows>, TestError> {
        self.execute_query_with_params(insert_query, params).await
    }

    /// Parse JSON results from query execution
    /// Returns a map of field names to arrays of JSON objects
    /// Queries return JSON in a column named "result" which contains an object with query field names as keys
    pub async fn parse_query_results(
        &self,
        rows: Vec<libsql::Rows>,
    ) -> Result<HashMap<String, Vec<JsonValue>>, TestError> {
        let mut result = HashMap::new();

        for mut rows_set in rows {
            // Get column names
            let column_count = rows_set.column_count();
            if column_count == 0 {
                continue;
            }

            // Process rows - queries return JSON in a column named "result"
            while let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                // Look for a column named "result" (or use the first column if "result" doesn't exist)
                let mut found_result = false;
                for i in 0..column_count {
                    let col_name = rows_set.column_name(i).ok_or(TestError::NoQueryFound)?;

                    // Get the JSON value from the column
                    if let Ok(json_str) = row.get::<String>(i as i32) {
                        if let Ok(json_value) = serde_json::from_str::<JsonValue>(&json_str) {
                            // If this is the "result" column, it contains an object with query field names as keys
                            if col_name == "result" {
                                if let JsonValue::Object(obj) = json_value {
                                    // Extract each field from the result object
                                    for (key, value) in obj {
                                        match value {
                                            JsonValue::Array(arr) => {
                                                result
                                                    .entry(key.clone())
                                                    .or_insert_with(Vec::new)
                                                    .extend(arr);
                                            }
                                            JsonValue::Object(_) => {
                                                result
                                                    .entry(key.clone())
                                                    .or_insert_with(Vec::new)
                                                    .push(value);
                                            }
                                            _ => {
                                                result
                                                    .entry(key.clone())
                                                    .or_insert_with(Vec::new)
                                                    .push(value);
                                            }
                                        }
                                    }
                                    found_result = true;
                                    break; // Found result column, no need to check other columns
                                }
                            } else if !found_result && i == column_count - 1 {
                                // Fallback: if we've checked all columns and no "result" column found,
                                // treat the column name as field name (for backwards compatibility)
                                match json_value {
                                    JsonValue::Array(arr) => {
                                        result
                                            .entry(col_name.to_string())
                                            .or_insert_with(Vec::new)
                                            .extend(arr);
                                    }
                                    JsonValue::Object(_) => {
                                        result
                                            .entry(col_name.to_string())
                                            .or_insert_with(Vec::new)
                                            .push(json_value);
                                    }
                                    _ => {
                                        result
                                            .entry(col_name.to_string())
                                            .or_insert_with(Vec::new)
                                            .push(json_value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Execute raw SQL and return rows
    pub async fn execute_raw(&self, sql: &str) -> Result<libsql::Rows, TestError> {
        let conn = self.db.connect().map_err(TestError::Database)?;
        conn.query(sql, ()).await.map_err(TestError::Database)
    }

    /// Seed the database with standard test data
    /// This creates users, posts, and accounts based on the schema
    pub async fn seed_standard_data(&self) -> Result<(), TestError> {
        // Check if schema has User record (table names are lowercase in context)
        if self.context.tables.contains_key("user") {
            // Check if User has status field (for schemas with union types)
            let has_status = self
                .context
                .tables
                .get("user")
                .map(|t| {
                    t.record.fields.iter().any(|f| match f {
                        pyre::ast::Field::Column(col) => col.name == "status",
                        _ => false,
                    })
                })
                .unwrap_or(false);

            if has_status {
                // Insert users with status
                let insert_user = r#"
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
                self.execute_insert_with_params(insert_user, params).await?;

                let mut params = HashMap::new();
                params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
                params.insert(
                    "status".to_string(),
                    libsql::Value::Text("Inactive".to_string()),
                );
                self.execute_insert_with_params(insert_user, params).await?;

                let mut params = HashMap::new();
                params.insert(
                    "name".to_string(),
                    libsql::Value::Text("Charlie".to_string()),
                );
                params.insert(
                    "status".to_string(),
                    libsql::Value::Text("Special".to_string()),
                );
                self.execute_insert_with_params(insert_user, params).await?;
            } else {
                // Insert users without status
                let insert_user = r#"
                    insert CreateUser($name: String) {
                        user {
                            name = $name
                        }
                    }
                "#;

                let mut params = HashMap::new();
                params.insert("name".to_string(), libsql::Value::Text("Alice".to_string()));
                self.execute_insert_with_params(insert_user, params).await?;

                let mut params = HashMap::new();
                params.insert("name".to_string(), libsql::Value::Text("Bob".to_string()));
                self.execute_insert_with_params(insert_user, params).await?;
            }
        }

        // Check if schema has Post record (table names are lowercase in context)
        if self.context.tables.contains_key("post") {
            let insert_post = r#"
                insert CreatePost($title: String, $content: String, $authorId: Int) {
                    post {
                        title = $title
                        content = $content
                        authorId = $authorId
                    }
                }
            "#;

            let mut params = HashMap::new();
            params.insert(
                "title".to_string(),
                libsql::Value::Text("First Post".to_string()),
            );
            params.insert(
                "content".to_string(),
                libsql::Value::Text("Content here".to_string()),
            );
            params.insert("authorId".to_string(), libsql::Value::Integer(1));
            self.execute_insert_with_params(insert_post, params).await?;

            let mut params = HashMap::new();
            params.insert(
                "title".to_string(),
                libsql::Value::Text("Second Post".to_string()),
            );
            params.insert(
                "content".to_string(),
                libsql::Value::Text("More content".to_string()),
            );
            params.insert("authorId".to_string(), libsql::Value::Integer(1));
            self.execute_insert_with_params(insert_post, params).await?;
        }

        // Check if schema has Account record (table names are lowercase in context)
        if self.context.tables.contains_key("account") {
            let insert_account = r#"
                insert CreateAccount($userId: Int, $name: String, $status: String) {
                    account {
                        userId = $userId
                        name = $name
                        status = $status
                    }
                }
            "#;

            let mut params = HashMap::new();
            params.insert("userId".to_string(), libsql::Value::Integer(1));
            params.insert(
                "name".to_string(),
                libsql::Value::Text("Account 1".to_string()),
            );
            params.insert(
                "status".to_string(),
                libsql::Value::Text("active".to_string()),
            );
            self.execute_insert_with_params(insert_account, params)
                .await?;
        }

        Ok(())
    }
}

fn format_errors(schema_source: &str, errors: &[error::Error]) -> String {
    errors
        .iter()
        .map(|e| error::format_error(schema_source, e))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace $param_name placeholders with ? for positional parameters
/// Parameters are replaced in the order they appear in param_names
fn replace_params_positional(sql: &str, param_names: &[String]) -> String {
    let mut result = sql.to_string();
    for name in param_names {
        // Replace $name with ? for positional parameters
        // We need to be careful to replace whole parameter names, not substrings
        result = result.replace(&format!("${}", name), "?");
    }
    result
}
