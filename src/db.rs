use crate::ast;
use crate::ext::string;
use crate::format;
use libsql;
use serde;

// List all tables
// Returns list of string
const LIST_TABLES: &str = "SELECT name FROM sqlite_master WHERE type='table';";

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

fn table_indices(table_name: &str) -> String {
    format!("PRAGMA index_list({})", table_name)
}

#[derive(Debug)]
pub struct Introspection {
    pub schema: ast::Schema,
    pub warnings: Vec<Warning>,
}

#[derive(Debug)]
pub enum Warning {
    WasManuallyModified(String),
}

pub async fn local(connection: &str) -> Result<libsql::Database, libsql::Error> {
    // libsql::Builder::new_local(":memory:").build().await
    libsql::Builder::new_local(connection).build().await
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

fn read_serialization_type(serialization_type: &str) -> ast::SerializationType {
    match serialization_type {
        "INTEGER" => ast::SerializationType::Integer,
        "TEXT" => ast::SerializationType::Text,
        "REAL" => ast::SerializationType::Real,
        "BLOB" => ast::SerializationType::BlobWithSchema("Schema".to_string()),
        "DATETIME" => ast::SerializationType::Text,
        _ => ast::SerializationType::Text,
    }
}

fn to_formatted_tablename(table_name: &str) -> String {
    string::capitalize(&to_formatted_tablename_lower(table_name))
}

fn to_formatted_tablename_lower(table_name: &str) -> String {
    string::snake_to_camel_and_singular(table_name)
}

pub async fn introspect(db: &libsql::Database) -> Result<Introspection, libsql::Error> {
    match db.connect() {
        Ok(conn) => {
            let args: Vec<String> = vec![];
            let table_list_result = conn.query(LIST_TABLES, args).await;
            let mut definitions: Vec<ast::Definition> = vec![];

            match table_list_result {
                Ok(mut table_rows) => {
                    while let Some(row) = table_rows.next().await? {
                        let table = libsql::de::from_row::<Table>(&row).unwrap();
                        if table.name == "sqlite_sequence" {
                            // Built in table, skip pls
                            continue;
                        }
                        // print!("{:?}\n", table);

                        // println!("{:?}", row);
                        let mut fields: Vec<ast::Field> = vec![];
                        fields.push(ast::Field::FieldDirective(ast::FieldDirective::TableName(
                            table.name.clone(),
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
                                    // if fk.table == table.name && fk.from == table_info.name {
                                    fields.push(ast::Field::FieldDirective(
                                        ast::FieldDirective::Link(ast::LinkDetails {
                                            link_name: to_formatted_tablename_lower(&fk.table),
                                            local_ids: vec![fk.from],

                                            foreign_tablename: to_formatted_tablename(&fk.table),
                                            foreign_ids: vec![fk.to],
                                        }),
                                    ));
                                    // }
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

                            // Capture fields

                            fields.push(ast::Field::Column(ast::Column {
                                name: table_info.name,
                                type_: to_column_type(&table_info.column_type),
                                nullable: table_info.notnull,
                                serialization_type: read_serialization_type(
                                    &table_info.column_type,
                                ),
                                directives,
                            }));
                        }

                        definitions.push(ast::Definition::Record {
                            name: to_formatted_tablename(&table.name),
                            fields,
                        })
                    }

                    let mut schema = ast::Schema { definitions };
                    format::schema(&mut schema);

                    Ok(Introspection {
                        schema,
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
