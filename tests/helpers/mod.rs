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

// Re-export for use in tests
pub use pyre::generate::sql::to_sql::SqlAndParams;

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
            .map_err(|errors| TestError::TypecheckError(format_errors(&errors)))?;

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

        for sql_stmt in migration_sql {
            match sql_stmt {
                SqlAndParams::Sql(sql) => {
                    tx.execute_batch(&sql).await.map_err(TestError::Database)?;
                }
                SqlAndParams::SqlWithParams { sql, args } => {
                    // Convert args to libsql::Value
                    let values: Vec<libsql::Value> =
                        args.into_iter().map(|s| libsql::Value::Text(s)).collect();
                    tx.execute(&sql, libsql::params_from_iter(values))
                        .await
                        .map_err(TestError::Database)?;
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

    /// Seed the database with raw SQL
    pub async fn seed(&self, sql: &str) -> Result<(), TestError> {
        let conn = self.db.connect().map_err(TestError::Database)?;
        conn.execute_batch(sql).await.map_err(TestError::Database)?;
        Ok(())
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
            .map_err(|errors| TestError::TypecheckError(format_errors(&errors)))?;

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

    /// Execute an insert query and return results
    pub async fn execute_insert(&self, insert_query: &str) -> Result<Vec<libsql::Rows>, TestError> {
        self.execute_query(insert_query).await
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

            let mut column_names = Vec::new();
            for i in 0..column_count {
                column_names.push(
                    rows_set
                        .column_name(i)
                        .ok_or(TestError::NoQueryFound)?
                        .to_string(),
                );
            }

            // Process rows - queries return JSON in the first column
            while let Some(row) = rows_set.next().await.map_err(TestError::Database)? {
                for (i, col_name) in column_names.iter().enumerate() {
                    // Get the JSON value from the column
                    if let Ok(json_str) = row.get::<String>(i as i32) {
                        if let Ok(json_value) = serde_json::from_str::<JsonValue>(&json_str) {
                            // The JSON might be an array or an object
                            match json_value {
                                JsonValue::Array(arr) => {
                                    result
                                        .entry(col_name.clone())
                                        .or_insert_with(Vec::new)
                                        .extend(arr);
                                }
                                JsonValue::Object(_) => {
                                    result
                                        .entry(col_name.clone())
                                        .or_insert_with(Vec::new)
                                        .push(json_value);
                                }
                                _ => {
                                    result
                                        .entry(col_name.clone())
                                        .or_insert_with(Vec::new)
                                        .push(json_value);
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
}

#[derive(Debug)]
pub enum TestError {
    Io(std::io::Error),
    Database(libsql::Error),
    ParseError(String),
    TypecheckError(String),
    InvalidPath,
    NoQueryFound,
    NoQueryInfoFound,
}

fn format_errors(errors: &[error::Error]) -> String {
    errors
        .iter()
        .map(|e| format!("{:?}", e))
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
