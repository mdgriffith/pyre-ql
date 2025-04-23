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

pub fn is_empty(diff: &Diff) -> bool {
    diff.added.is_empty() && diff.removed.is_empty() && diff.modified_records.is_empty()
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

fn default_value_to_sql(default: &crate::ast::DefaultValue) -> Option<String> {
    match default {
        crate::ast::DefaultValue::Now => Some("unixepoch()".to_string()),
        crate::ast::DefaultValue::Value(value) => match value {
            crate::ast::QueryValue::String((_, s)) => Some(format!("'{}'", s)),
            crate::ast::QueryValue::Int((_, i)) => Some(i.to_string()),
            crate::ast::QueryValue::Float((_, f)) => Some(f.to_string()),
            crate::ast::QueryValue::Bool((_, b)) => Some(if *b { "1" } else { "0" }.to_string()),
            crate::ast::QueryValue::Null(_) => Some("null".to_string()),
            _ => None, // Other types not supported as defaults
        },
    }
}

fn add_fields(
    context: &crate::typecheck::Context,
    fields: &Vec<crate::ast::Field>,
    columns: &mut Vec<crate::db::introspect::ColumnInfo>,
    field_namespace: Option<String>,
    seen_fields: &mut std::collections::HashSet<String>,
    force_nullable: bool,
) {
    for f in fields {
        if let crate::ast::Field::Column(col) = f {
            let column_name = if let Some(namespace) = &field_namespace {
                format!("{}__{}", namespace, col.name)
            } else {
                col.name.clone()
            };

            if seen_fields.contains(&column_name) {
                continue;
            }
            seen_fields.insert(column_name.clone());

            let default_value = col.directives.iter().find_map(|d| match d {
                crate::ast::ColumnDirective::Default { value, .. } => {
                    return Some(format!("({})", default_value_to_sql(value).unwrap()));
                }
                _ => None,
            });

            match &col.serialization_type {
                crate::ast::SerializationType::Concrete(concrete) => {
                    columns.push(crate::db::introspect::ColumnInfo {
                        cid: 0,
                        name: column_name,
                        column_type: concrete.to_sql_type(),
                        notnull: !force_nullable && !col.nullable,
                        default_value,
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
                                columns.push(crate::db::introspect::ColumnInfo {
                                    cid: 0,
                                    name: column_name.clone(),
                                    column_type: crate::ast::ConcreteSerializationType::Text
                                        .to_sql_type(),
                                    notnull: !force_nullable && !col.nullable,
                                    default_value: None,
                                    pk: false,
                                });

                                for variant in variants {
                                    if let Some(var_fields) = &variant.fields {
                                        // Pass true for force_nullable for variant fields
                                        add_fields(
                                            context,
                                            var_fields,
                                            columns,
                                            Some(column_name.clone()),
                                            seen_fields,
                                            true, // Force nullable for variant fields
                                        );
                                    }
                                }
                            }
                            _ => {
                                columns.push(crate::db::introspect::ColumnInfo {
                                    cid: 0,
                                    name: column_name,
                                    column_type: typename.clone(),
                                    notnull: !force_nullable && !col.nullable,
                                    default_value: None,
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
}

fn create_table_from_fields(
    context: &crate::typecheck::Context,
    name: &str,
    fields: &Vec<crate::ast::Field>,
) -> crate::db::introspect::Table {
    let mut columns = Vec::new();
    add_fields(
        context,
        fields,
        &mut columns,
        None,
        &mut std::collections::HashSet::new(),
        false,
    );

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
