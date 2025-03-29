use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod to_sql;
// Define a type to represent the diff of two schemas
#[derive(Debug, Serialize, Deserialize)]
pub struct Diff {
    pub added: Vec<crate::db::introspect::Table>,
    pub removed: Vec<crate::db::introspect::Table>,
    pub modified_records: Vec<DetailedRecordDiff>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetailedRecordDiff {
    pub name: String,
    pub changes: Vec<RecordChange>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RecordChange {
    AddedField(crate::db::introspect::ColumnInfo),
    RemovedField(crate::db::introspect::ColumnInfo),
    ModifiedField { name: String, changes: ColumnDiff },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnDiff {
    pub type_changed: Option<(String, String)>, // (old_type, new_type)
    pub nullable_changed: Option<(bool, bool)>, // (old_nullable, new_nullable)
}

pub fn diff(
    schema: &crate::ast::Schema,
    introspection: &crate::db::introspect::Introspection,
) -> Diff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified_records = Vec::new();

    // Create lookup maps for faster comparison - need to extract tables from all schema files
    let mut schema_tables: std::collections::HashMap<_, _> = HashMap::new();
    for file in &schema.files {
        for def in &file.definitions {
            if let crate::ast::Definition::Record { name, fields, .. } = def {
                schema_tables.insert(name, fields);
            }
        }
    }

    let intro_tables: std::collections::HashMap<_, _> =
        introspection.tables.iter().map(|t| (&t.name, t)).collect();

    // Find added and modified tables
    for (name, schema_fields) in &schema_tables {
        match intro_tables.get(*name) {
            None => {
                let table = create_table_from_fields(name, schema_fields);
                added.push(table);
            }
            Some(intro_table) => {
                let schema_table = create_table_from_fields(name, schema_fields);
                if let Some(record_diff) = compare_record(&schema_table, intro_table) {
                    modified_records.push(record_diff);
                }
            }
        }
    }

    // Find removed tables
    for name in intro_tables.keys() {
        if !schema_tables.contains_key(name) {
            removed.push(intro_tables[name].clone());
        }
    }

    Diff {
        added,
        removed,
        modified_records,
    }
}

// Helper function to create a Table from fields
fn create_table_from_fields(
    name: &str,
    fields: &Vec<crate::ast::Field>,
) -> crate::db::introspect::Table {
    let columns = fields
        .iter()
        .filter_map(|f| {
            if let crate::ast::Field::Column(col) = f {
                Some(crate::db::introspect::ColumnInfo {
                    cid: 0, // This will be set by SQLite
                    name: col.name.clone(),
                    column_type: col.type_.clone(),
                    notnull: !col.nullable,
                    dflt_value: None, // We don't track default values in the diff
                    pk: col
                        .directives
                        .iter()
                        .any(|d| matches!(d, crate::ast::ColumnDirective::PrimaryKey)),
                })
            } else {
                None
            }
        })
        .collect();

    crate::db::introspect::Table {
        name: name.to_string(),
        columns,
        foreign_keys: vec![],
    }
}

// Helper function to compare record fields
fn compare_record(
    schema_table: &crate::db::introspect::Table,
    intro_table: &crate::db::introspect::Table,
) -> Option<DetailedRecordDiff> {
    let mut changes = Vec::new();

    let schema_columns: std::collections::HashMap<_, _> =
        schema_table.columns.iter().map(|c| (&c.name, c)).collect();

    let intro_columns: std::collections::HashMap<_, _> =
        intro_table.columns.iter().map(|c| (&c.name, c)).collect();

    // Find added and modified columns
    for (name, schema_col) in &schema_columns {
        match intro_columns.get(name) {
            None => changes.push(RecordChange::AddedField((*schema_col).clone())),
            Some(intro_col) => {
                let type_changed = if schema_col.column_type != intro_col.column_type {
                    Some((
                        intro_col.column_type.clone(),
                        schema_col.column_type.clone(),
                    ))
                } else {
                    None
                };

                let nullable_changed = if schema_col.notnull != intro_col.notnull {
                    Some((intro_col.notnull, schema_col.notnull))
                } else {
                    None
                };

                if type_changed.is_some() || nullable_changed.is_some() {
                    changes.push(RecordChange::ModifiedField {
                        name: name.to_string(),
                        changes: ColumnDiff {
                            type_changed,
                            nullable_changed,
                        },
                    });
                }
            }
        }
    }

    // Find removed columns
    for name in intro_columns.keys() {
        if !schema_columns.contains_key(name) {
            changes.push(RecordChange::RemovedField(intro_columns[name].clone()));
        }
    }

    if changes.is_empty() {
        None
    } else {
        Some(DetailedRecordDiff {
            name: schema_table.name.clone(),
            changes,
        })
    }
}
