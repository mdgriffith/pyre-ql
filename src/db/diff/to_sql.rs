use super::*;

pub fn to_sql(diff: &Diff) -> String {
    let mut sql_statements = Vec::new();

    // Handle removed tables first (to avoid foreign key conflicts)
    for table in &diff.removed {
        sql_statements.push(format!("drop table if exists \"{}\"", table.name));
    }

    // Handle added tables
    for table in &diff.added {
        let columns: Vec<String> = table.columns.iter().map(column_definition).collect();

        let mut create_stmt = format!(
            "create table \"{}\" (\n  {}\n)",
            table.name,
            columns.join(",\n  ")
        );

        // Add foreign key constraints if any exist
        if !table.foreign_keys.is_empty() {
            create_stmt = create_stmt.trim_end_matches(')').to_string();
            for fk in &table.foreign_keys {
                create_stmt.push_str(&format!(
                    ",\n  foreign key ({}) references {}({})",
                    fk.from, fk.table, fk.to
                ));
            }
            create_stmt.push(')');
        }

        sql_statements.push(create_stmt);
    }

    // Handle modified tables
    for record_diff in &diff.modified_records {
        for change in &record_diff.changes {
            match change {
                RecordChange::AddedField(column) => {
                    sql_statements.push(format!(
                        "alter table \"{}\" add column {}",
                        record_diff.name,
                        column_definition(column)
                    ));
                }
                RecordChange::RemovedField(column) => {
                    sql_statements.push(format!(
                        "alter table \"{}\" drop column \"{}\"",
                        record_diff.name, column.name
                    ));
                }
                RecordChange::ModifiedField { name, changes } => {
                    // For SQLite, we can't directly modify columns, so we need to recreate the table
                    // This would require more complex migration logic
                    sql_statements.push(format!(
                        "-- WARNING: Column modification for `{}`.`{}` requires table recreation",
                        record_diff.name, name
                    ));
                }
            }
        }
    }

    sql_statements.join(";\n") + ";"
}

fn column_definition(column: &crate::db::introspect::ColumnInfo) -> String {
    let mut def = format!("`{}` {}", column.name, column.column_type);

    if column.pk {
        def.push_str(" primary key autoincrement");
    }

    if column.notnull {
        def.push_str(" not null");
    }

    if let Some(default_value) = &column.default_value {
        def.push_str(&format!(" default {}", default_value));
    }

    def
}
