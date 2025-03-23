use crate::ast::{
    collect_links, get_foreign_tablename, get_tablename, is_field_primary_key, link_identity,
    Column, ColumnDirective, ConcreteSerializationType, DefaultValue, Definition, QueryValue,
    Schema, VectorType,
};
use crate::diff::{DetailedRecordDiff, DetailedTaggedDiff, RecordChange, SchemaDiff, TaggedChange};
use crate::ext::string;
use crate::typecheck;

pub fn to_sql(context: &typecheck::Context, schema: &Schema, diff: &SchemaDiff) -> String {
    let mut sql_statements = Vec::new();

    // Handle added definitions
    for definition in &diff.added {
        sql_statements.push(add_definition_sql(context, schema, definition));
    }

    // Handle removed definitions
    for definition in &diff.removed {
        sql_statements.push(remove_definition_sql(definition));
    }

    // Handle modified records
    for record_diff in &diff.modified_records {
        sql_statements.push(handle_modified_record(record_diff));
    }

    // Handle modified taggeds
    for tagged_diff in &diff.modified_taggeds {
        sql_statements.push(handle_modified_tagged(tagged_diff));
    }

    sql_statements.join("\n")
}

fn add_definition_sql(
    context: &typecheck::Context,
    schema: &Schema,
    definition: &Definition,
) -> String {
    match definition {
        Definition::Record { name, fields, .. } => {
            let name = get_tablename(&name, &fields);
            let fields_sql: Vec<String> = typecheck::to_sql_column_info(context, fields)
                .iter()
                .map(|f| {
                    format!(
                        "{} {}{}{}",
                        string::quote(&f.name),
                        serialization_to_string(&f.type_),
                        if f.nullable { "" } else { " not null" },
                        column_directive_list_to_string(&f, &f.directives),
                    )
                })
                .collect();

            let link_constraints: Vec<String> = collect_links(fields)
                .iter()
                .filter_map(|link| {
                    // Skip the constraint if the local_id is referencing the primary key of this table.
                    if is_field_primary_key(&link.local_ids, &fields) {
                        return None;
                    }

                    let foreign_table = get_foreign_tablename(&schema, &link);
                    Some(format!(
                        "constraint {} foreign key ({}) references {} ({})",
                        string::quote(&link_identity(&name, &link)),
                        string::quote(&link.local_ids.join(", ")),
                        string::quote(&foreign_table),
                        string::quote(&link.foreign.fields.join(", "))
                    ))
                })
                .collect();

            format!(
                "create table {} (\n    {}{}\n);",
                string::quote(&name),
                fields_sql.join(",\n    "),
                if link_constraints.is_empty() {
                    "\n".to_string()
                } else {
                    format!(",\n    {}", link_constraints.join(",\n    "))
                }
            )
        }
        _ => "".to_string(),
    }
}

fn column_directive_list_to_string(
    column: &typecheck::SqlColumnInfo,
    directives: &Vec<ColumnDirective>,
) -> String {
    if directives.is_empty() {
        return "".to_string();
    }

    let directive_strings: Vec<String> = directives
        .iter()
        .map(|dir| column_directive_to_string(column, dir))
        .collect();

    format!(" {}", directive_strings.join(" "))
}

fn column_directive_to_string(
    column: &typecheck::SqlColumnInfo,
    directive: &ColumnDirective,
) -> String {
    match directive {
        ColumnDirective::PrimaryKey => "primary key autoincrement".to_string(),
        ColumnDirective::Unique => "unique".to_string(),
        ColumnDirective::Default { id, value } => match value {
            DefaultValue::Now => match column.type_ {
                ConcreteSerializationType::Date => "default current_date".to_string(),
                ConcreteSerializationType::DateTime => "default (unixepoch())".to_string(),
                _ => "".to_string(),
            },

            DefaultValue::Value(value) => {
                format!("default {}", value_to_string(&value))
            }
        },
    }
}

fn value_to_string(value: &QueryValue) -> String {
    match value {
        QueryValue::Fn(_) => "".to_string(),       // not allowed
        QueryValue::Variable(_) => "".to_string(), // not allowed
        QueryValue::String((_, value)) => format!("'{}'", value),
        QueryValue::Int((_, value)) => value.to_string(),
        QueryValue::Float((_, value)) => value.to_string(),
        QueryValue::Bool((_, value)) => value.to_string(),
        QueryValue::Null(_) => "null".to_string(),
        QueryValue::LiteralTypeValue((_, details)) => details.name.clone(),
    }
}

fn serialization_to_string(serialization_type: &ConcreteSerializationType) -> String {
    match serialization_type {
        ConcreteSerializationType::Integer => "INTEGER".to_string(),
        ConcreteSerializationType::Real => "REAL".to_string(),
        ConcreteSerializationType::Text => "TEXT".to_string(),
        ConcreteSerializationType::Blob => "BLOB".to_string(),
        ConcreteSerializationType::JsonB => "JSON_BLOB".to_string(),
        ConcreteSerializationType::Date => "TEXT".to_string(),
        ConcreteSerializationType::DateTime => "INTEGER".to_string(),
        ConcreteSerializationType::VectorBlob {
            vector_type,
            dimensionality,
        } => match vector_type {
            VectorType::Float64 => format!("F64_BLOB({})", dimensionality),
            VectorType::Float32 => format!("F32_BLOB({})", dimensionality),
            VectorType::Float16 => format!("F16_BLOB({})", dimensionality),
            VectorType::BFloat16 => format!("FB16_BLOB({})", dimensionality),
            VectorType::Float8 => format!("F8_BLOB({})", dimensionality),
            VectorType::Float1 => format!("F1BIT_BLOB({})", dimensionality),
        },
    }
}

fn remove_definition_sql(definition: &Definition) -> String {
    match definition {
        Definition::Record { name, .. } => format!("drop table if exists {};", name),
        _ => "".to_string(),
    }
}

fn handle_modified_record(record_diff: &DetailedRecordDiff) -> String {
    let mut sql_statements = Vec::new();

    for change in &record_diff.changes {
        match change {
            RecordChange::AddedField(field) => {
                sql_statements.push(format!(
                    "alter table {} add column {} {};",
                    record_diff.name, field.name, field.type_
                ));
            }
            RecordChange::RemovedField(field) => {
                // SQLite does not support dropping columns directly. A workaround involves
                // creating a new table without the column and copying data.
                sql_statements.push(format!(
                    "-- Removing column {} from table {} requires manual migration.",
                    field.name, record_diff.name
                ));
            }
            RecordChange::ModifiedField { name, changes } => {
                // sql_statements.push(format!(
                //     "-- Modifying column {} in table {} requires manual migration from {} to {}.",
                //     name, record_diff.name, old.type_, new.type_
                // ));
                sql_statements.push(format!(
                    "-- Modifying column {} in table {} requires manual migration.",
                    name, record_diff.name
                ));
            }
        }
    }

    sql_statements.join("\n")
}

fn handle_modified_tagged(tagged_diff: &DetailedTaggedDiff) -> String {
    let mut sql_statements = Vec::new();

    for change in &tagged_diff.changes {
        match change {
            TaggedChange::AddedVariant(variant) => {
                sql_statements.push(format!(
                    "-- Add variant: {:?} to tagged: {}",
                    variant, tagged_diff.name
                ));
                // Convert `variant` to a valid SQL statement to add the variant.
                // This is just a placeholder.
                sql_statements.push(format!("/* TODO: Add SQL for adding variant */"));
            }
            TaggedChange::RemovedVariant(variant) => {
                sql_statements.push(format!(
                    "-- Remove variant: {:?} from tagged: {}",
                    variant, tagged_diff.name
                ));
                // Convert `variant` to a valid SQL statement to remove the variant.
                // This is just a placeholder.
                sql_statements.push(format!("/* TODO: Add SQL for removing variant */"));
            }
            TaggedChange::ModifiedVariant { name, old, new } => {
                sql_statements.push(format!(
                    "-- Modify variant: {} in tagged: {} from {:?} to {:?}",
                    name, tagged_diff.name, old, new
                ));
                // Convert `old` and `new` to a valid SQL statement to modify the variant.
                // This is just a placeholder.
                sql_statements.push(format!("/* TODO: Add SQL for modifying variant */"));
            }
        }
    }

    sql_statements.join("\n")
}
