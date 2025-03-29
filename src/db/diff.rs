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
    context: &crate::typecheck::Context,
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
                let table_name = crate::ast::get_tablename(name, fields);
                schema_tables.insert(table_name, (name, fields));
            }
        }
    }

    let intro_tables: std::collections::HashMap<_, _> =
        introspection.tables.iter().map(|t| (&t.name, t)).collect();

    // Find added and modified tables
    for (table_name, (_record_name, schema_fields)) in &schema_tables {
        match intro_tables.get(table_name) {
            None => {
                let table = create_table_from_fields(context, table_name, schema_fields);
                added.push(table);
            }
            Some(intro_table) => {
                let schema_table = create_table_from_fields(context, table_name, schema_fields);
                if let Some(record_diff) = compare_record(&schema_table, intro_table) {
                    modified_records.push(record_diff);
                }
            }
        }
    }

    // Find removed tables
    for (table_name, intro_table) in intro_tables {
        if !schema_tables.contains_key(table_name) {
            removed.push((*intro_table).clone());
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
    context: &crate::typecheck::Context,
    name: &str,
    fields: &Vec<crate::ast::Field>,
) -> crate::db::introspect::Table {
    let mut columns = Vec::new();

    for f in fields {
        if let crate::ast::Field::Column(col) = f {
            match &col.serialization_type {
                crate::ast::SerializationType::Concrete(_) => {
                    // For concrete types, create a single column
                    columns.push(crate::db::introspect::ColumnInfo {
                        cid: 0, // This will be set by SQLite
                        name: col.name.clone(),
                        column_type: col.type_.clone(),
                        notnull: !col.nullable,
                        dflt_value: None, // We don't track default values in the diff
                        pk: col
                            .directives
                            .iter()
                            .any(|d| matches!(d, crate::ast::ColumnDirective::PrimaryKey)),
                    });
                }
                crate::ast::SerializationType::FromType(typename) => {
                    if let Some((_, type_)) = context.types.get(typename) {
                        match type_ {
                            crate::typecheck::Type::OneOf { variants } => {
                                // Add discriminator column only for OneOf types
                                columns.push(crate::db::introspect::ColumnInfo {
                                    cid: 0,
                                    name: col.name.clone(),
                                    column_type: "Text".to_string(),
                                    notnull: !col.nullable,
                                    dflt_value: None,
                                    pk: false,
                                });

                                // Track seen fields to avoid duplicates
                                let mut seen_fields = std::collections::HashSet::new();

                                for variant in variants {
                                    if let Some(var_fields) = &variant.fields {
                                        for var_field in var_fields {
                                            if let crate::ast::Field::Column(var_col) = var_field {
                                                let field_name =
                                                    format!("{}__{}", col.name, var_col.name);

                                                // Only add the field if we haven't seen it before
                                                if !seen_fields.contains(&field_name) {
                                                    seen_fields.insert(field_name.clone());
                                                    columns.push(
                                                        crate::db::introspect::ColumnInfo {
                                                            cid: 0,
                                                            name: field_name,
                                                            column_type: var_col.type_.clone(),
                                                            notnull: false, // Variant fields are always technically nullable
                                                            dflt_value: None,
                                                            pk: false,
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // For other types (Integer, Float, String, Record), create a single column
                            _ => {
                                columns.push(crate::db::introspect::ColumnInfo {
                                    cid: 0,
                                    name: col.name.clone(),
                                    column_type: typename.clone(),
                                    notnull: !col.nullable,
                                    dflt_value: None,
                                    pk: col.directives.iter().any(|d| {
                                        matches!(d, crate::ast::ColumnDirective::PrimaryKey)
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    crate::db::introspect::Table {
        name: crate::ast::get_tablename(name, fields),
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
