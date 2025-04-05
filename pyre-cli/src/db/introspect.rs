use libsql;
use pyre::db::introspect::{
    ColumnInfo, DbTable, ForeignKey, Introspection, Migration, MigrationRun, MigrationState, Table,
    GET_SCHEMA, LIST_MIGRATIONS, LIST_TABLES, MIGRATION_TABLE, SCHEMA_TABLE,
};

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

            let mut has_schema_table = false;
            let mut has_migration_table = false;

            // Get schema
            match table_list_result {
                Ok(mut table_rows) => {
                    while let Some(row) = table_rows.next().await? {
                        let table = libsql::de::from_row::<DbTable>(&row).unwrap();
                        if table.name == "sqlite_sequence"
                            || table.name == MIGRATION_TABLE
                            || table.name == SCHEMA_TABLE
                        {
                            if table.name == SCHEMA_TABLE {
                                has_schema_table = true;
                            }
                            if table.name == MIGRATION_TABLE {
                                has_migration_table = true;
                            }
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

                    let migration_state = if has_migration_table {
                        get_migration_state(&conn).await?
                    } else {
                        MigrationState::NoMigrationTable
                    };

                    // Query for schema
                    let mut schema = None;
                    if has_schema_table {
                        let args: Vec<String> = vec![];
                        let mut schema_result = conn.query(GET_SCHEMA, args).await?;
                        if let Some(row) = schema_result.next().await? {
                            // Deserialize the schema JSON string into Schema struct
                            match libsql::de::from_row::<String>(&row) {
                                Ok(schema_str) => match serde_json::from_str(&schema_str) {
                                    Ok(schema_json) => schema = Some(schema_json),
                                    Err(_) => (),
                                },
                                Err(_) => (),
                            }
                        }
                    }

                    Ok(Introspection {
                        tables,
                        migration_state,
                        schema,
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
