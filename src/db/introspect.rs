use libsql;
use serde;
use serde::{Deserialize, Serialize};

/*
Introspection is used to drive migrations.


First introspection is run.

Then we diff `Introspection` with `Schema` which produces a `Diff`, which can be turned into a
Migration SQL.




*/

#[derive(Debug, Serialize, Deserialize)]
pub struct Introspection {
    pub tables: Vec<Table>,
    pub migrations_recorded: Vec<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ForeignKey {
    pub id: usize,
    pub seq: usize,

    // Target table
    pub table: String,
    pub from: String,
    pub to: String,
    pub on_update: String,
    pub on_delete: String,

    #[serde(rename = "match")]
    pub match_: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ColumnInfo {
    pub cid: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: String,
    #[serde(deserialize_with = "deserialize_notnull")]
    pub notnull: bool,
    pub dflt_value: Option<String>,

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

pub async fn introspect(
    db: &libsql::Database,
    namespace: &str,
) -> Result<Introspection, libsql::Error> {
    match db.connect() {
        Ok(conn) => {
            let args: Vec<String> = vec![];
            let table_list_result = conn.query(crate::db::LIST_TABLES, args).await;
            let mut tables: Vec<Table> = vec![];
            let mut migrations_recorded: Vec<String> = vec![];
            let mut has_migrations_table = false;

            match table_list_result {
                Ok(mut table_rows) => {
                    while let Some(row) = table_rows.next().await? {
                        let table = libsql::de::from_row::<DbTable>(&row).unwrap();
                        if table.name == "sqlite_sequence" {
                            // Built in table, skip pls
                            continue;
                        } else if table.name == crate::db::MIGRATION_TABLE {
                            // Built in table, skip pls
                            has_migrations_table = true;
                            continue;
                        }
                        // print!("{:?}\n", table);

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

                    // Read Migration Table
                    if has_migrations_table {
                        let args: Vec<String> = vec![];
                        let migration_list_result =
                            conn.query(crate::db::LIST_MIGRATIONS, args).await;
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

                    Ok(Introspection {
                        tables,
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
