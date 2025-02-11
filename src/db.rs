use crate::ast;
use crate::error;
use crate::ext::string;
use crate::format;
use crate::parser;
use libsql;
use serde;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MIGRATION_TABLE: &str = "_pyre_migrations";

// List all tables
// Returns list of string
const LIST_TABLES: &str = "SELECT name FROM sqlite_master WHERE type='table';";

const LIST_MIGRATIONS: &str = "SELECT name FROM _pyre_migrations;";

// [
//   {
//     "cid": "0",
//     "name": "id",
//     "type": "INTEGER",
//     "notnull": "1",
//     "dflt_value": null,
//     "pk": "1"
//   },
//   {
//     "cid": "1",
//     "name": "createdAt",
//     "type": "DATETIME",
//     "notnull": "1",
//     "dflt_value": "CURRENT_TIMESTAMP",
//     "pk": "0"
//   },
// ]
fn table_info(table_name: &str) -> String {
    format!("PRAGMA table_info({})", table_name)
}

// [
//   {
//     "id": "0",
//     "seq": "0",
//     "table": "rulebooks",
//     "from": "rulebookId",
//     "to": "id",
//     "on_update": "CASCADE",
//     "on_delete": "CASCADE",
//     "match": "NONE"
//   },
//   {
//     "id": "1",
//     "seq": "0",
//     "table": "users",
//     "from": "userId",
//     "to": "id",
//     "on_update": "CASCADE",
//     "on_delete": "CASCADE",
//     "match": "NONE"
//   }
// ]
fn foreign_key_list(table_name: &str) -> String {
    format!("PRAGMA foreign_key_list({})", table_name)
}

// fn table_indices(table_name: &str) -> String {
//     format!("PRAGMA index_list({})", table_name)
// }

#[derive(Debug)]
pub struct Introspection {
    pub schema: ast::Schema,
    pub migrations_recorded: Vec<String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug)]
pub enum Warning {
    WasManuallyModified(String),
}

#[derive(Debug)]
pub enum DbError {
    AuthTokenRequired,
    EnvVarNotFound(String),
    DatabaseError(libsql::Error),
}

impl DbError {
    pub fn format_error(&self) -> String {
        match self {
            DbError::AuthTokenRequired => error::format_custom_error(
                "Authentication Error",
                "Authentication token is required",
            ),
            DbError::EnvVarNotFound(var) => error::format_custom_error(
                "Unknown Environment Variable",
                &format!("Environment variable {} not found", var),
            ),
            DbError::DatabaseError(e) => {
                error::format_custom_error("Database Error", &format!("Database error: {:?}", e))
            }
        }
    }
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
        let connected_result = libsql::Builder::new_local(db_value).build().await;
        match connected_result {
            Ok(connected) => return Ok(connected),
            Err(e) => return Err(DbError::DatabaseError(e)),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct Table {
    name: String,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct ForeignKey {
    id: usize,
    seq: usize,
    table: String,
    from: String,
    to: String,
    on_update: String,
    on_delete: String,

    #[serde(rename = "match")]
    match_: String,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct ColumnInfo {
    cid: usize,
    name: String,
    #[serde(rename = "type")]
    column_type: String,
    #[serde(deserialize_with = "deserialize_notnull")]
    notnull: bool,
    dflt_value: Option<String>,

    #[serde(deserialize_with = "deserialize_notnull")]
    pk: bool,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct MigrationRun {
    name: String,
}

fn read_serialization_type(serialization_type: &str) -> ast::SerializationType {
    let type_;
    if serialization_type.contains("INT") {
        type_ = ast::ConcreteSerializationType::Integer;
    } else {
        match serialization_type {
            "INTEGER" => type_ = ast::ConcreteSerializationType::Integer,
            "TEXT" => type_ = ast::ConcreteSerializationType::Text,
            "REAL" => type_ = ast::ConcreteSerializationType::Real,
            "BLOB" => type_ = ast::ConcreteSerializationType::Blob,
            "DATETIME" => type_ = ast::ConcreteSerializationType::Text,
            _ => type_ = ast::ConcreteSerializationType::Text,
        }
    };

    ast::SerializationType::Concrete(type_)
}

fn to_formatted_tablename(table_name: &str) -> String {
    string::capitalize(&to_formatted_tablename_lower(table_name))
}

fn to_formatted_tablename_lower(table_name: &str) -> String {
    string::snake_to_camel_and_singular(table_name)
}

pub async fn introspect(
    db: &libsql::Database,
    namespace: &str,
) -> Result<Introspection, libsql::Error> {
    match db.connect() {
        Ok(conn) => {
            let args: Vec<String> = vec![];
            let table_list_result = conn.query(LIST_TABLES, args).await;
            let mut definitions: Vec<ast::Definition> = vec![];
            let mut migrations_recorded: Vec<String> = vec![];
            let mut has_migrations_table = false;

            match table_list_result {
                Ok(mut table_rows) => {
                    while let Some(row) = table_rows.next().await? {
                        let table = libsql::de::from_row::<Table>(&row).unwrap();
                        if table.name == "sqlite_sequence" {
                            // Built in table, skip pls
                            continue;
                        } else if table.name == MIGRATION_TABLE {
                            // Built in table, skip pls
                            has_migrations_table = true;
                            continue;
                        }
                        // print!("{:?}\n", table);

                        // println!("{:?}", row);
                        let mut fields: Vec<ast::Field> = vec![];
                        fields.push(ast::Field::FieldDirective(ast::FieldDirective::TableName(
                            (ast::empty_range(), table.name.clone()),
                        )));

                        // FKs
                        let fk_args: Vec<String> = vec![];
                        let mut foreign_key_list_result = conn
                            .query(&foreign_key_list(&table.name), fk_args)
                            .await
                            .unwrap();
                        while let Some(fk_row) = foreign_key_list_result.next().await? {
                            let fk_result = libsql::de::from_row::<ForeignKey>(&fk_row);
                            match fk_result {
                                Ok(fk) => {
                                    fields.push(ast::Field::FieldDirective(
                                        ast::FieldDirective::Link(ast::LinkDetails {
                                            link_name: to_formatted_tablename_lower(&fk.table),
                                            local_ids: vec![fk.from],

                                            foreign: ast::Qualified {
                                                schema: namespace.to_string(),
                                                table: to_formatted_tablename(&fk.table),
                                                fields: vec![fk.to],
                                            },

                                            start_name: None,
                                            end_name: None,
                                        }),
                                    ));
                                }
                                Err(e) => {
                                    println!("{:?}", e);
                                }
                            }
                        }

                        // All columns
                        let args2: Vec<String> = vec![];
                        let mut table_info_result =
                            conn.query(&table_info(&table.name), args2).await.unwrap();

                        while let Some(table_info_row) = table_info_result.next().await? {
                            let table_info =
                                libsql::de::from_row::<ColumnInfo>(&table_info_row).unwrap();
                            // print!("{:?}\n", table_info);

                            let mut directives: Vec<ast::ColumnDirective> = vec![];

                            if table_info.pk {
                                directives.push(ast::ColumnDirective::PrimaryKey);
                            }

                            match table_info.dflt_value {
                                None => (),
                                Some(str) => {
                                    if str.to_lowercase() == "current_timestamp"
                                        || str.to_lowercase() == "unixepoch()"
                                    {
                                        directives.push(ast::ColumnDirective::Default(
                                            ast::DefaultValue::Now,
                                        ));
                                    } else if str == "true" {
                                        directives.push(ast::ColumnDirective::Default(
                                            ast::DefaultValue::Value(ast::QueryValue::Bool((
                                                ast::empty_range(),
                                                true,
                                            ))),
                                        ));
                                    } else if str == "false" {
                                        directives.push(ast::ColumnDirective::Default(
                                            ast::DefaultValue::Value(ast::QueryValue::Bool((
                                                ast::empty_range(),
                                                false,
                                            ))),
                                        ));
                                    } else if str.starts_with("'") {
                                        let my_string = str.trim_matches('\'');

                                        directives.push(ast::ColumnDirective::Default(
                                            ast::DefaultValue::Value(ast::QueryValue::String((
                                                ast::empty_range(),
                                                my_string.to_string(),
                                            ))),
                                        ));
                                    } else {
                                        let parsed = parser::parse_number(parser::Text::new_extra(
                                            &str,
                                            parser::placeholder_context(),
                                        ));
                                        match parsed {
                                            Ok((_, val)) => {
                                                directives.push(ast::ColumnDirective::Default(
                                                    ast::DefaultValue::Value(val),
                                                ));
                                            }
                                            Err(err) => {
                                                println!("Unrecognized default {}", str)
                                            }
                                        }
                                    }
                                }
                            }

                            // Capture fields

                            fields.push(ast::Field::Column(ast::Column {
                                name: table_info.name,
                                type_: to_column_type(&table_info.column_type),
                                nullable: !table_info.notnull,
                                serialization_type: read_serialization_type(
                                    &table_info.column_type,
                                ),
                                directives,
                                start: None,
                                end: None,
                                start_name: None,
                                end_name: None,
                                start_typename: None,
                                end_typename: None,
                            }));
                        }

                        definitions.push(ast::Definition::Record {
                            name: to_formatted_tablename(&table.name),
                            fields,
                            start: None,
                            end: None,
                            start_name: None,
                            end_name: None,
                        })
                    }

                    // Read Migration Table
                    if has_migrations_table {
                        let args: Vec<String> = vec![];
                        let migration_list_result = conn.query(LIST_MIGRATIONS, args).await;
                        match migration_list_result {
                            Ok(mut migration_rows) => {
                                while let Some(row) = migration_rows.next().await? {
                                    let migration =
                                        libsql::de::from_row::<MigrationRun>(&row).unwrap();
                                    migrations_recorded.push(migration.name);
                                }
                            }
                            Err(e) => {
                                println!("Error: {}", e);
                                return Err(e);
                            }
                        }
                    }

                    // Prepare Schema
                    let mut schema = ast::Schema {
                        namespace: namespace.to_string(),
                        session: None,
                        files: vec![ast::SchemaFile {
                            path: "schema.pyre".to_string(),
                            definitions,
                        }],
                    };
                    format::schema(&mut schema);

                    Ok(Introspection {
                        schema,
                        migrations_recorded,
                        warnings: vec![],
                    })
                }
                Err(e) => {
                    println!("Error: {}", e);
                    Err(e)
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            Err(e)
        }
    }
}

fn to_column_type(column_type: &str) -> String {
    if column_type.contains("INT") {
        "Int".to_string()
    } else {
        match column_type {
            "INTEGER" => "Int".to_string(),
            "TEXT" => "String".to_string(),
            "REAL" => "Float".to_string(),
            "BLOB" => "Blob".to_string(),
            "DATETIME" => "DateTime".to_string(),
            "BOOLEAN" => "Bool".to_string(),
            _ => column_type.to_string(),
        }
    }
}

fn deserialize_notnull<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let i: i32 = serde::Deserialize::deserialize(deserializer)?;
    match i {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(serde::de::Error::custom("unexpected value")),
    }
}

// Migrations

#[derive(Debug)]
pub enum MigrationError {
    SqlError(libsql::Error),
    MigrationReadIoError(std::io::Error, PathBuf),
}

impl MigrationError {
    pub fn format_error(&self) -> String {
        match self {
            MigrationError::SqlError(sql_error) => error::format_libsql_error(sql_error),
            MigrationError::MigrationReadIoError(io_error, path) => error::format_custom_error(
                "Migration Read Error",
                &format!(
                    "I was looking for migrations in {},\nbut ran into the following issue:\n\n{}",
                    error::yellow_if(true, &path.display().to_string()),
                    io_error
                ),
            ),
        }
    }
}

pub async fn migrate(db: &libsql::Database, migration_folder: &Path) -> Result<(), MigrationError> {
    // Read migration directory
    let migration_file_result = read_migrations(migration_folder);
    match migration_file_result {
        Err(err) => {
            return Err(MigrationError::MigrationReadIoError(
                err,
                migration_folder.to_path_buf(),
            ));
        }
        Ok(migration_files) => {
            let introspection_result = introspect(&db, ast::DEFAULT_SCHEMANAME).await;
            match introspection_result {
                Err(err) => {
                    return Err(MigrationError::SqlError(err));
                }
                Ok(introspection) => {
                    // Read
                    let conn_result = db.connect();
                    match conn_result {
                        Err(err) => {
                            return Err(MigrationError::SqlError(err));
                        }
                        Ok(conn) => {
                            create_migration_table_if_not_exists(&conn).await.unwrap();

                            for (migration_filename, migration_contents) in
                                migration_files.file_contents
                            {
                                // Check if migration has been run
                                if introspection
                                    .migrations_recorded
                                    .contains(&migration_filename)
                                {
                                    continue;
                                }

                                // Run migration
                                let tx = conn
                                    .transaction_with_behavior(
                                        libsql::TransactionBehavior::Immediate,
                                    )
                                    .await
                                    .unwrap();

                                tx.execute_batch(&migration_contents).await.unwrap();
                                record_migration(&tx, &migration_filename).await.unwrap();

                                tx.commit().await.unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn create_migration_table_if_not_exists(
    conn: &libsql::Connection,
) -> Result<(), libsql::Error> {
    let create_migration_table = &format!(
        r#"
CREATE TABLE IF NOT EXISTS {} (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);"#,
        crate::ext::string::quote(MIGRATION_TABLE)
    );
    conn.execute_batch(create_migration_table).await
}

async fn record_migration(
    conn: &libsql::Connection,
    migration_name: &str,
) -> Result<u64, libsql::Error> {
    let insert_migration = &format!(
        r#"INSERT INTO {} (name) VALUES (?);"#,
        crate::ext::string::quote(MIGRATION_TABLE)
    );
    conn.execute(insert_migration, libsql::params![migration_name])
        .await
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
    pub file_map: HashMap<String, bool>,
    pub file_contents: Vec<(String, String)>,
}

pub fn read_migrations(migration_folder: &Path) -> Result<Migrations, std::io::Error> {
    // Initialize the HashMap and Vec
    let mut file_map: HashMap<String, bool> = HashMap::new();
    let mut file_contents: Vec<(String, String)> = Vec::new();

    for entry in fs::read_dir(migration_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let migrate_file_path = path.join("migration.sql");
            if migrate_file_path.is_file() {
                if let Some(folder_name) = path.file_name().and_then(|name| name.to_str()) {
                    // Insert the folder name into the HashMap with a value of false
                    file_map.insert(folder_name.to_string(), false);

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

    Ok(Migrations {
        file_map,
        file_contents,
    })
}
