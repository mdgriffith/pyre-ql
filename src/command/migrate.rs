use std::io;
use std::path::Path;

use super::shared::{check_namespace_requirements, Options};
use crate::db;

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

    let connection_result = db::connect(&database.to_string(), auth).await;
    match connection_result {
        Ok(conn) => {
            let migration_result = db::migrate(&conn, &namespace_migration_dir).await;
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
    Ok(())
}
