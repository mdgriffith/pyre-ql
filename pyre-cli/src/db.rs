use pyre::ast;

use libsql;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod error;
pub mod introspect;

#[derive(Debug)]
pub enum DbError {
    AuthTokenRequired,
    EnvVarNotFound(String),
    DatabaseError(libsql::Error),
    LocalFilesystemError(std::io::Error, PathBuf),
}

impl DbError {
    pub fn format_error(&self) -> String {
        match self {
            DbError::AuthTokenRequired => pyre::error::format_custom_error(
                "Authentication Error",
                "Authentication token is required",
            ),
            DbError::EnvVarNotFound(var) => pyre::error::format_custom_error(
                "Unknown Environment Variable",
                &format!("Environment variable {} not found", var),
            ),
            DbError::DatabaseError(e) => pyre::error::format_custom_error(
                "Database Error",
                &format!("Database error: {:?}", e),
            ),
            DbError::LocalFilesystemError(e, path) => pyre::error::format_custom_error(
                "Local Database Path Error",
                &format!(
                    "Failed to create local database directory {}: {}",
                    path.display(),
                    e
                ),
            ),
        }
    }
}

fn ensure_local_db_parent_exists(db_path: &str) -> Result<(), DbError> {
    if db_path == ":memory:" {
        return Ok(());
    }

    let path = Path::new(db_path);
    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() || parent.exists() {
            return Ok(());
        }

        fs::create_dir_all(parent)
            .map_err(|e| DbError::LocalFilesystemError(e, parent.to_path_buf()))?;
    }

    Ok(())
}

fn parse_arg_or_env(arg: &str) -> Result<String, DbError> {
    if arg.starts_with('$') {
        let env_var_name = &arg[1..];
        env::var(env_var_name).map_err(|_| DbError::EnvVarNotFound(env_var_name.to_string()))
    } else {
        Ok(arg.to_string())
    }
}

pub async fn connect(
    db: &String,
    maybe_auth_token: &Option<String>,
) -> Result<libsql::Database, DbError> {
    let db_value = parse_arg_or_env(&db)?;

    if db_value.starts_with("http://")
        || db_value.starts_with("https://")
        || db_value.starts_with("libsql://")
    {
        // Remote database
        match maybe_auth_token {
            None => return Err(DbError::AuthTokenRequired),
            Some(token) => {
                let token_value = parse_arg_or_env(&token)?;
                let connected_result = libsql::Builder::new_remote(db_value, token_value)
                    .build()
                    .await;
                match connected_result {
                    Ok(connected) => return Ok(connected),
                    Err(e) => return Err(DbError::DatabaseError(e)),
                }
            }
        }
    } else {
        ensure_local_db_parent_exists(&db_value)?;

        let connected_result = libsql::Builder::new_local(db_value).build().await;
        match connected_result {
            Ok(connected) => return Ok(connected),
            Err(e) => return Err(DbError::DatabaseError(e)),
        }
    }
}

// Migrations

#[derive(Debug)]
pub enum MigrationError {
    SqlError(libsql::Error),
    MigrationReadIoError(std::io::Error, PathBuf),
    NoMigrationsFound(PathBuf),
}

impl MigrationError {
    pub fn format_error(&self) -> String {
        match self {
            MigrationError::SqlError(sql_error) => error::format_libsql_error(sql_error),
            MigrationError::MigrationReadIoError(io_error, path) => {
                pyre::error::format_custom_error(
                    "Migration Read Error",
                    &format!(
                    "I was looking for migrations in {},\nbut ran into the following issue:\n\n{}",
                    pyre::error::yellow_if(true, &path.display().to_string()),
                    io_error
                ),
                )
            }
            MigrationError::NoMigrationsFound(path) => pyre::error::format_custom_error(
                "No Migrations Found",
                &format!(
                    "No migrations were found in {}.\n\nRun `pyre migration --db <database> init` to generate your first migration.",
                    pyre::error::yellow_if(true, &path.display().to_string())
                ),
            ),
        }
    }
}

/*

This doesn't do any checking on the schema or the migrations, it just runs them.


*/
pub async fn migrate(
    db: &libsql::Database,
    schema: &ast::Schema,
    migration_folder: &Path,
) -> Result<(), MigrationError> {
    // Read migration directory
    let migration_file_result = read_migration_folder(migration_folder);
    match migration_file_result {
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Err(MigrationError::NoMigrationsFound(
                    migration_folder.to_path_buf(),
                ));
            }

            return Err(MigrationError::MigrationReadIoError(
                err,
                migration_folder.to_path_buf(),
            ));
        }
        Ok(migration_files) => {
            if migration_files.file_contents.is_empty() {
                return Err(MigrationError::NoMigrationsFound(
                    migration_folder.to_path_buf(),
                ));
            }

            // Read
            let conn_result = db.connect();
            match conn_result {
                Err(err) => {
                    return Err(MigrationError::SqlError(err));
                }
                Ok(conn) => {
                    // Create migration tables using centralized constants
                    // Format table names with quotes for safety
                    let migration_table_sql = pyre::db::migrate::CREATE_MIGRATION_TABLE.replace(
                        pyre::db::migrate::MIGRATION_TABLE,
                        &pyre::ext::string::quote(pyre::db::migrate::MIGRATION_TABLE),
                    );
                    let schema_table_sql = pyre::db::migrate::CREATE_SCHEMA_TABLE.replace(
                        pyre::db::migrate::SCHEMA_TABLE,
                        &pyre::ext::string::quote(pyre::db::migrate::SCHEMA_TABLE),
                    );
                    conn.execute_batch(&format!(
                        "{};\n\n{};",
                        migration_table_sql, schema_table_sql
                    ))
                    .await
                    .unwrap();

                    let migration_state = introspect::get_migration_state(&conn).await.unwrap();

                    // Use centralized migration planning logic
                    let migration_plan = pyre::db::migrate::plan_file_based_migrations(
                        &migration_files.file_contents,
                        &migration_state,
                        schema,
                    );

                    // Run migration
                    let tx = conn
                        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
                        .await
                        .unwrap();

                    // Execute migrations that need to be run
                    for (migration_filename, migration_contents) in migration_plan.migrations_to_run
                    {
                        tx.execute_batch(&migration_contents).await.unwrap();

                        // Record migration using centralized constant
                        // INSERT_MIGRATION_SUCCESS requires (name, sql, finished_at)
                        // where finished_at is set to unixepoch() automatically
                        let insert_sql = pyre::db::migrate::INSERT_MIGRATION_SUCCESS.replace(
                            pyre::db::migrate::MIGRATION_TABLE,
                            &pyre::ext::string::quote(pyre::db::migrate::MIGRATION_TABLE),
                        );
                        tx.execute(
                            &insert_sql,
                            libsql::params![migration_filename, migration_contents],
                        )
                        .await
                        .unwrap();
                    }

                    // Insert schema using centralized SQL generation
                    tx.execute(
                        &migration_plan.insert_schema_sql,
                        libsql::params![migration_plan.schema_string],
                    )
                    .await
                    .unwrap();

                    tx.commit().await.unwrap();
                }
            }
        }
    }

    Ok(())
}

pub fn read_migration_items(migration_folder: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut migration_items: Vec<String> = Vec::new();

    for entry in fs::read_dir(migration_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(folder_name) = path.file_name().and_then(|name| name.to_str()) {
                migration_items.push(folder_name.to_string());
            }
        }
    }

    Ok(migration_items)
}

pub struct Migrations {
    pub file_contents: Vec<(String, String)>,
}

pub fn read_migration_folder(migration_folder: &Path) -> Result<Migrations, std::io::Error> {
    let mut file_contents: Vec<(String, String)> = Vec::new();

    for entry in fs::read_dir(migration_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let migrate_file_path = path.join("migration.sql");
            if migrate_file_path.is_file() {
                if let Some(folder_name) = path.file_name().and_then(|name| name.to_str()) {
                    // Read the file contents
                    let mut file = fs::File::open(&migrate_file_path)?;
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)?;

                    // Store the folder name and contents in the Vec
                    file_contents.push((folder_name.to_string(), contents));
                }
            }
        }
    }

    Ok(Migrations { file_contents })
}
