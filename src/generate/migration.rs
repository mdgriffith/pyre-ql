use crate::ast::{
    collect_columns, get_tablename, ColumnDirective, Definition, Field, FieldDirective,
    SerializationType, Variant,
};
use crate::diff::{DetailedRecordDiff, DetailedTaggedDiff, RecordChange, SchemaDiff, TaggedChange};

pub fn to_sql(diff: &SchemaDiff) -> String {
    let mut sql_statements = Vec::new();

    // Handle added definitions
    for definition in &diff.added {
        sql_statements.push(add_definition_sql(definition));
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

fn add_definition_sql(definition: &Definition) -> String {
    match definition {
        Definition::Record { name, fields } => {
            let name = get_tablename(&name, &fields);
            let fields_sql: Vec<String> = collect_columns(fields)
                .iter()
                .map(|f| {
                    format!(
                        "{} {}{}{}{}",
                        f.name,
                        serialization_to_string(&f.serialization_type),
                        if f.nullable { "" } else { " NOT NULL" },
                        column_directive_list_to_string(&f.directives),
                        serialization_comment_to_string(&f.serialization_type)
                    )
                })
                .collect();
            format!(
                "create table {} (\n    {}\n);",
                name,
                fields_sql.join(",\n    ")
            )
        }
        _ => "".to_string(),
    }
}

fn column_directive_list_to_string(directives: &Vec<ColumnDirective>) -> String {
    if directives.is_empty() {
        return "".to_string();
    }

    let directive_strings: Vec<String> =
        directives.iter().map(columne_directive_to_string).collect();

    format!(" {}", directive_strings.join(" "))
}

fn columne_directive_to_string(directive: &ColumnDirective) -> String {
    match directive {
        ColumnDirective::PrimaryKey => "primary key".to_string(),
        ColumnDirective::Unique => "unique".to_string(),
    }
}

fn serialization_to_string(serialization_type: &SerializationType) -> String {
    match serialization_type {
        SerializationType::Integer => "integer".to_string(),
        SerializationType::Real => "real".to_string(),
        SerializationType::Text => "text".to_string(),
        SerializationType::BlobWithSchema(schema) => "blob".to_string(),
    }
}

fn serialization_comment_to_string(serialization_type: &SerializationType) -> String {
    match serialization_type {
        SerializationType::BlobWithSchema(schema) => format!(" -- {}", schema).to_string(),
        _ => "".to_string(),
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
            RecordChange::ModifiedField { name, old, new } => {
                sql_statements.push(format!(
                    "-- Modifying column {} in table {} requires manual migration from {} to {}.",
                    name, record_diff.name, old.type_, new.type_
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
