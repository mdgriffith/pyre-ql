use std::io;
use std::path::Path;

use super::shared::{check_namespace_requirements, parse_database_schemas, Options};
use crate::ast;
use crate::db;
use crate::error;
use crate::filesystem;
use crate::typecheck;

pub async fn migrate<'a>(
    options: &'a Options<'a>,
    database: &str,
    auth: &Option<String>,
    migration_dir: &str,
    namespace: &Option<String>,
) -> io::Result<()> {
    check_namespace_requirements(&namespace, &options);
    let namespace_migration_dir = match namespace {
        Some(ns) => Path::new(migration_dir).join(ns),
        None => Path::new(migration_dir).to_path_buf(),
    };

    // Get schema
    let paths = filesystem::collect_filepaths(&options.in_dir)?;
    let all_schemas = parse_database_schemas(&paths)?;

    let real_namespace = match namespace {
        Some(ns) => ns,
        None => &ast::DEFAULT_SCHEMANAME.to_string(),
    };

    // Get exactly one schema based on namespace or default
    let schema = all_schemas
        .schemas
        .iter()
        .find(|schema| schema.namespace == *real_namespace)
        .ok_or_else(|| {
            eprintln!("Error: No schema found for namespace '{}'", real_namespace);
            std::process::exit(1);
        });
    let schema = schema.unwrap(); // Safe to unwrap after error handling above

    // Typecheck schemas

    match typecheck::check_schema(&all_schemas) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths);
        }
        Ok(_context) => {
            let connection_result = db::connect(&database.to_string(), auth).await;
            match connection_result {
                Ok(conn) => {
                    let migration_result =
                        db::migrate(&conn, &schema, &namespace_migration_dir).await;
                    match migration_result {
                        Ok(()) => {
                            println!("Migration finished!");
                        }
                        Err(migration_error) => {
                            println!("{}", migration_error.format_error());
                        }
                    }
                }
                Err(err) => {
                    println!("{}", err.format_error());
                }
            }
        }
    }
    Ok(())
}
