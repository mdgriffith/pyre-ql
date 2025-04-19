use crate::ast;
use crate::error;
use crate::parser;
use crate::typecheck;
use serde;
use serde::{Deserialize, Serialize};

pub mod to_schema;

pub const MIGRATION_TABLE: &str = "_pyre_migrations";

pub const SCHEMA_TABLE: &str = "_pyre_schema";

// List all tables
// Returns list of string
pub const LIST_TABLES: &str = "SELECT name FROM sqlite_master WHERE type='table';";

pub const LIST_MIGRATIONS: &str = "SELECT name FROM _pyre_migrations;";

// Add this near the top with other constants
pub const GET_SCHEMA: &str = "SELECT schema FROM _pyre_schema LIMIT 1;";

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

#[derive(Debug)]
pub enum SchemaResult {
    FailedToParse {
        source: String,
        errors: Vec<error::Error>,
    },
    FailedToTypecheck {
        schema: ast::Schema,
        errors: Vec<error::Error>,
    },
    Success {
        schema: ast::Schema,
        context: typecheck::Context,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntrospectionRaw {
    pub tables: Vec<Table>,
    pub migration_state: MigrationState,
    pub schema_source: String,
}

#[derive(Debug)]
pub struct Introspection {
    pub tables: Vec<Table>,
    pub migration_state: MigrationState,
    pub schema: SchemaResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKey>,
}

// Intermediates

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DbTable {
    pub name: String,
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
pub struct MigrationRun {
    pub name: String,
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

pub fn from_raw(raw: IntrospectionRaw) -> Introspection {
    if raw.schema_source.is_empty() {
        let context = typecheck::empty_context();
        return Introspection {
            tables: raw.tables,
            migration_state: raw.migration_state,
            schema: SchemaResult::Success {
                schema: ast::Schema::default(),
                context,
            },
        };
    }

    let mut schema = ast::Schema {
        namespace: ast::DEFAULT_SCHEMANAME.to_string(),
        session: None,
        files: vec![],
    };

    // Attempt to parse the schema source
    let schema_result = match parser::run("schema.pyre", &raw.schema_source, &mut schema) {
        Ok(()) => {
            // Create a Database from the schema
            let database = ast::Database {
                schemas: vec![schema.clone()],
            };

            // Typecheck the schema
            match typecheck::check_schema(&database) {
                Ok(context) => SchemaResult::Success { schema, context },
                Err(errors) => SchemaResult::FailedToTypecheck { schema, errors },
            }
        }
        Err(err) => {
            let source = raw.schema_source.clone();
            if let Some(parsing_error) = parser::convert_parsing_error(err) {
                SchemaResult::FailedToParse {
                    source,
                    errors: vec![parsing_error],
                }
            } else {
                SchemaResult::FailedToParse {
                    source,
                    errors: vec![error::Error {
                        error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                            expecting: error::Expecting::PyreFile,
                        }),
                        filepath: "schema.pyre".to_string(),
                        locations: vec![],
                    }],
                }
            }
        }
    };

    Introspection {
        tables: raw.tables,
        migration_state: raw.migration_state,
        schema: schema_result,
    }
}
