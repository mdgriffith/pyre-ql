use libsql;
use serde;
use serde::{Deserialize, Serialize};

pub mod to_schema;

pub const MIGRATION_TABLE: &str = "_pyre_migrations";

// List all tables
// Returns list of string
const LIST_TABLES: &str = "SELECT name FROM sqlite_master WHERE type='table';";

const LIST_MIGRATIONS: &str = "SELECT name FROM _pyre_migrations;";

/*
Introspection is used to drive migrations.


First introspection is run.

Then we diff `Introspection` with `Schema` which produces a `Diff`, which can be turned into a
Migration SQL.




*/

#[derive(Debug, Serialize, Deserialize)]
pub struct Migration {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MigrationState {
    NoMigrationTable,
    MigrationTable { migrations: Vec<Migration> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Introspection {
    pub tables: Vec<Table>,
    pub migration_state: MigrationState,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Warning {
    WasManuallyModified(String),
}

// Intermediates

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct DbTable {
    name: String,
}

/*



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


*/
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(try_from = "String")]
pub enum ForeignKeyAction {
    #[serde(rename = "CASCADE")]
    Cascade,
    #[serde(rename = "RESTRICT")]
    Restrict,
    #[serde(rename = "NO ACTION")]
    NoAction,
    #[serde(rename = "SET NULL")]
    SetNull,
    #[serde(rename = "SET DEFAULT")]
    SetDefault,
}

impl TryFrom<String> for ForeignKeyAction {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "CASCADE" => Ok(ForeignKeyAction::Cascade),
            "RESTRICT" => Ok(ForeignKeyAction::Restrict),
            "NO ACTION" => Ok(ForeignKeyAction::NoAction),
            "SET NULL" => Ok(ForeignKeyAction::SetNull),
            "SET DEFAULT" => Ok(ForeignKeyAction::SetDefault),
            _ => Err(format!("Unknown foreign key action: {}", value)),
        }
    }
}

/// Specifies how NULL values in foreign keys are handled during constraint checking.
/// Note: In current SQLite versions, this is effectively a no-op as only SIMPLE
/// matching behavior is implemented, regardless of the specified value.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(try_from = "String")]
pub enum ForeignKeyMatch {
    /// SIMPLE matching allows a foreign key to be NULL unless the parent key is a composite key
    /// and only some columns of the foreign key are NULL. If the foreign key is a composite key
    /// and any column is NULL, then all columns must be NULL for the constraint to be satisfied.
    Simple,

    /// FULL matching requires that either all or none of the foreign key columns be NULL.
    /// If any foreign key column is NULL, then all columns must be NULL for the constraint
    /// to be satisfied. If all foreign key columns are non-NULL, they must match a parent key.
    Full,

    /// NONE matching allows any column in the foreign key to be NULL, regardless of whether
    /// other columns in the foreign key are NULL or not. This is the default behavior if
    /// no MATCH clause is specified.
    None,
}

impl TryFrom<String> for ForeignKeyMatch {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "SIMPLE" => Ok(ForeignKeyMatch::Simple),
            "FULL" => Ok(ForeignKeyMatch::Full),
            "NONE" => Ok(ForeignKeyMatch::None),
            _ => Err(format!("Unknown foreign key match: {}", value)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ForeignKey {
    pub id: usize,
    pub seq: usize,
    pub table: String,
    pub from: String,
    pub to: String,
    pub on_update: ForeignKeyAction,
    pub on_delete: ForeignKeyAction,
    #[serde(rename = "match")]
    pub match_: ForeignKeyMatch,
}

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
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ColumnInfo {
    pub cid: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: String,
    #[serde(deserialize_with = "deserialize_notnull")]
    pub notnull: bool,

    #[serde(rename = "dflt_value")]
    pub default_value: Option<String>,

    #[serde(deserialize_with = "deserialize_notnull")]
    pub pk: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct MigrationRun {
    name: String,
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

pub async fn get_migration_state(
    conn: &libsql::Connection,
) -> Result<MigrationState, libsql::Error> {
    let args: Vec<String> = vec![];
    let table_list_result = conn.query(LIST_TABLES, args).await?;
    let mut has_migrations_table = false;

    let mut table_rows = table_list_result;
    while let Some(row) = table_rows.next().await? {
        let table = libsql::de::from_row::<DbTable>(&row).unwrap();
        if table.name == MIGRATION_TABLE {
            has_migrations_table = true;
            break;
        }
    }

    if !has_migrations_table {
        return Ok(MigrationState::NoMigrationTable);
    }

    let args: Vec<String> = vec![];
    let migration_list_result = conn.query(LIST_MIGRATIONS, args).await?;
    let mut migrations = Vec::new();

    let mut migration_rows = migration_list_result;
    while let Some(row) = migration_rows.next().await? {
        let migration_run = libsql::de::from_row::<MigrationRun>(&row).unwrap();
        migrations.push(Migration {
            name: migration_run.name,
        });
    }

    Ok(MigrationState::MigrationTable { migrations })
}

pub async fn introspect(db: &libsql::Database) -> Result<Introspection, libsql::Error> {
    match db.connect() {
        Ok(conn) => {
            let args: Vec<String> = vec![];
            let table_list_result = conn.query(LIST_TABLES, args).await;
            let mut tables: Vec<Table> = vec![];

            match table_list_result {
                Ok(mut table_rows) => {
                    while let Some(row) = table_rows.next().await? {
                        let table = libsql::de::from_row::<DbTable>(&row).unwrap();
                        if table.name == "sqlite_sequence" || table.name == MIGRATION_TABLE {
                            continue;
                        }

                        let mut foreign_keys: Vec<ForeignKey> = vec![];
                        let mut columns: Vec<ColumnInfo> = vec![];

                        let args: Vec<String> = vec![];
                        // List all Foreign Keys
                        let mut foreign_key_list_result = conn
                            .query(&format!("PRAGMA foreign_key_list({})", table.name), args)
                            .await
                            .unwrap();

                        while let Some(fk_row) = foreign_key_list_result.next().await? {
                            let fk_result = libsql::de::from_row::<ForeignKey>(&fk_row);
                            match fk_result {
                                Ok(fk) => foreign_keys.push(fk),
                                Err(e) => {
                                    println!("{:?}", e);
                                }
                            }
                        }

                        // All columns
                        let column_args: Vec<String> = vec![];
                        let mut table_info_result = conn
                            .query(&format!("PRAGMA table_info({})", table.name), column_args)
                            .await
                            .unwrap();

                        while let Some(table_info_row) = table_info_result.next().await? {
                            let column_info =
                                libsql::de::from_row::<ColumnInfo>(&table_info_row).unwrap();
                            // print!("{:?}\n", table_info);
                            columns.push(column_info);
                        }

                        tables.push(Table {
                            name: table.name,
                            columns,
                            foreign_keys,
                        })
                    }

                    let migration_state = get_migration_state(&conn).await?;

                    Ok(Introspection {
                        tables,
                        migration_state,
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
