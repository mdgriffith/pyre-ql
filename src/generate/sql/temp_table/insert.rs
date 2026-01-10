use crate::ast;
use crate::ext::string;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;

/*

See the temp_tables/mod.rs to see an overview of the sql strategy we want here.


The general algorithm.

1. Insert a value into the current table.
2. If there is a nested insert, create a temporary table with the name format of _temp_inserted_{table_field_alias}
    3. recursively generate for next nested insert.
4. Delete temp table.




*/

// Structure to track affected tables during inserts
struct AffectedTable {
    table_name: String,
    column_names: Vec<String>,
    temp_table_name: String,
}

pub fn insert_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    include_affected_rows: bool,
) -> Vec<to_sql::Prepared> {
    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut statements = to_sql::format_attach(query_info);
    statements.push(to_sql::ignore(initial_select(
        0,
        context,
        query,
        table,
        query_table_field,
    )));

    let parent_temp_table_name = &get_temp_table_name(&query_table_field);
    let mut temp_table_created = false;
    let mut multiple_table_inserts = false;
    let mut affected_tables: Vec<AffectedTable> = Vec::new();

    // Track parent table
    if include_affected_rows {
        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
        let column_names = collect_all_column_names(context, &table.record.fields);
        affected_tables.push(AffectedTable {
            table_name,
            column_names,
            temp_table_name: parent_temp_table_name.clone(),
        });

        // Create temp table for single-table inserts to track affected rows
        statements.push(to_sql::ignore(format!(
            "create temp table {} as\n  select last_insert_rowid() as id",
            parent_temp_table_name
        )));
        temp_table_created = true;
    }

    for query_field in all_query_fields.iter() {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                // We are inserting a link, so we need to do a nested insert
                let linked_table = typecheck::get_linked_table(context, &link).unwrap();
                multiple_table_inserts = true;

                if !temp_table_created {
                    statements.push(to_sql::ignore(format!(
                        "create temp table {} as\n  select last_insert_rowid() as id",
                        parent_temp_table_name
                    )));

                    temp_table_created = true;
                }

                // Track nested table
                if include_affected_rows {
                    let nested_temp_table_name = get_temp_table_name(query_field);
                    let linked_table_name =
                        ast::get_tablename(&linked_table.record.name, &linked_table.record.fields);
                    let linked_column_names =
                        collect_all_column_names(context, &linked_table.record.fields);
                    affected_tables.push(AffectedTable {
                        table_name: linked_table_name,
                        column_names: linked_column_names,
                        temp_table_name: nested_temp_table_name.clone(),
                    });
                }

                insert_linked(
                    0,
                    context,
                    query,
                    parent_temp_table_name,
                    linked_table,
                    query_field,
                    link,
                    &mut statements,
                    include_affected_rows,
                    &mut affected_tables,
                );
            }
            _ => (),
        }
    }

    // The final selection - wrap in JSON format
    if multiple_table_inserts {
        let mut final_statement = String::new();
        let query_field_name = &query_table_field.name;
        let primary_table_name = select::get_tablename(
            &select::TableAliasKind::Normal,
            table,
            &ast::get_aliased_name(&query_table_field),
        );
        
        // Create a CTE that selects the data we need, filtered by the temp table
        let table_alias = format!("selected__{}", ast::get_aliased_name(&query_table_field));
        final_statement.push_str("with ");
        final_statement.push_str(&table_alias);
        final_statement.push_str(" as (\n");
        final_statement.push_str("  select\n");
        
        // Select columns with explicit aliases so we can reference them in JSON generation
        let mut column_selections = Vec::new();
        for field in &query_table_field.fields {
            match field {
                ast::ArgField::Field(query_field) => {
                    if let Some(table_field) = table
                        .record
                        .fields
                        .iter()
                        .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    {
                        match table_field {
                            ast::Field::Column(_) => {
                                // Use the table alias 't' to reference columns
                                column_selections.push(format!(
                                    "    t.{} as {}",
                                    string::quote(&query_field.name),
                                    string::quote(&query_field.name)
                                ));
                            }
                            _ => {
                                // Skip links - they'll be handled in JSON generation
                            }
                        }
                    }
                }
                _ => continue,
            }
        }
        final_statement.push_str(&column_selections.join(",\n"));
        final_statement.push_str("\n  from ");
        final_statement.push_str(&primary_table_name);
        final_statement.push_str(" t");

        final_statement.push_str(&format!(
            "\n  where\n    t.rowid in (select id from {})\n",
            parent_temp_table_name
        ));
        final_statement.push_str(")\n");
        
        // Now wrap it in JSON format similar to final_select_formatted_as_json
        final_statement.push_str("select\n");
        final_statement.push_str("  json_object(\n");
        final_statement.push_str(&format!(
            "    '{}', coalesce(json_group_array(\n      json_object(\n",
            query_field_name
        ));
        
        // Generate JSON object fields - use actual field names from the selection
        let mut first_field = true;
        for field in &query_table_field.fields {
            match field {
                ast::ArgField::Field(query_field) => {
                    if let Some(table_field) = table
                        .record
                        .fields
                        .iter()
                        .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    {
                        let aliased_field_name = ast::get_aliased_name(query_field);
                        
                        match table_field {
                            ast::Field::Column(_) => {
                                if !first_field {
                                    final_statement.push_str(",\n");
                                }
                                // The CTE columns are selected by their field names
                                // Use the field name directly from the CTE
                                final_statement.push_str(&format!(
                                    "        '{}', {}.{}",
                                    aliased_field_name, table_alias, query_field.name
                                ));
                                first_field = false;
                            }
                            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                                // For links, we need to aggregate them properly
                                // Use the link name to reference the joined data
                                if !first_field {
                                    final_statement.push_str(",\n");
                                }
                                let linked_to_unique = if let Some(linked_table) =
                                    typecheck::get_linked_table(context, link)
                                {
                                    ast::linked_to_unique_field_with_record(link, &linked_table.record)
                                } else {
                                    ast::linked_to_unique_field(link)
                                };
                                if linked_to_unique {
                                    // Singular result - would need proper join handling
                                    final_statement.push_str(&format!(
                                        "        '{}', json('null')",
                                        aliased_field_name
                                    ));
                                } else {
                                    // Array result - would need proper aggregation
                                    final_statement.push_str(&format!(
                                        "        '{}', json('[]')",
                                        aliased_field_name
                                    ));
                                }
                                first_field = false;
                            }
                            _ => continue,
                        }
                    }
                }
                _ => continue,
            }
        }
        
        final_statement.push_str("\n      )\n    ), json('[]'))\n  ) as result\n");
        final_statement.push_str(&format!("from {}\n", table_alias));

        statements.push(to_sql::include(final_statement));
    }

    // Generate affected rows query if requested
    // Execute this BEFORE the final selection to avoid lock conflicts
    if include_affected_rows && !affected_tables.is_empty() {
        let affected_rows_sql =
            generate_affected_rows_query_for_inserts(context, query_info, &affected_tables);
        // Insert before the final selection if it exists
        if multiple_table_inserts {
            // Insert before the final selection (which is the last statement before this)
            let final_idx = statements.len() - 1;
            statements.insert(final_idx, to_sql::include(affected_rows_sql));
        } else {
            statements.push(to_sql::include(affected_rows_sql));
        }
    }

    // Drop temp tables when not tracking affected rows (no result sets = safe to drop).
    // When tracking affected rows, temp tables are automatically cleaned up when the batch's
    // logical connection closes (see docs/sql_remote.md). We don't drop them explicitly to
    // avoid lock errors from dropping while result sets are active.
    if multiple_table_inserts && !include_affected_rows {
        drop_temp_tables(query_table_field, &mut statements);
    }

    statements
}

fn drop_temp_tables(query_field: &ast::QueryField, statements: &mut Vec<to_sql::Prepared>) {
    statements.push(to_sql::ignore(drop_table(query_field)));

    // Only the primary field has a temp table created for it now
    // for arg_field in query_field.fields.iter() {
    //     match arg_field {
    //         ast::ArgField::Field(field) => {
    //             if !field.fields.is_empty() {
    //                 drop_temp_tables(field, statements);
    //             }
    //         }
    //         _ => continue,
    //     }
    // }
}

fn drop_table(query_field: &ast::QueryField) -> String {
    format!("drop table {}", &get_temp_table_name(&query_field))
}

pub fn get_temp_table_name(query_field: &ast::QueryField) -> String {
    format!("inserted_{}", &ast::get_aliased_name(&query_field))
}

pub fn initial_select(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
) -> String {
    let indent_str = " ".repeat(indent);
    let mut field_names: Vec<String> = Vec::new();

    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.append(&mut new_fieldnames.clone());

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    // Check if updatedAt field exists in table and is not explicitly set
    let has_updated_at_field = table
        .record
        .fields
        .iter()
        .any(|f| ast::has_fieldname(f, "updatedAt"));
    let updated_at_explicitly_set = all_query_fields.iter().any(|f| f.name == "updatedAt");

    if has_updated_at_field && !updated_at_explicitly_set {
        field_names.push("updatedAt".to_string());
    }

    let mut result = format!(
        "{}insert into {} ({})\n",
        indent_str,
        table_name,
        field_names.join(", ")
    );

    let values = &to_field_insert_values(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &all_query_fields,
    );

    let mut final_values = values.clone();
    if has_updated_at_field && !updated_at_explicitly_set {
        final_values.push("unixepoch()".to_string());
    }

    result.push_str(&format!(
        "{}values ({})",
        indent_str,
        final_values.join(", ")
    ));
    result
}

pub fn insert_linked(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,
    statements: &mut Vec<to_sql::Prepared>,
    include_affected_rows: bool,
    affected_tables: &mut Vec<AffectedTable>,
) {
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);
    let mut field_names: Vec<String> = Vec::new();

    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.push(link.foreign.fields.clone().join(", "));
    field_names.append(&mut new_fieldnames.clone());

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    // Check if updatedAt field exists in table and is not explicitly set
    let has_updated_at_field = table
        .record
        .fields
        .iter()
        .any(|f| ast::has_fieldname(f, "updatedAt"));
    let updated_at_explicitly_set = all_query_fields.iter().any(|f| f.name == "updatedAt");

    if has_updated_at_field && !updated_at_explicitly_set {
        field_names.push("updatedAt".to_string());
    }

    let mut insert_values = vec![];
    for local_id in &link.local_ids {
        insert_values.push(format!(
            "{}.{}",
            string::quote(parent_table_name),
            string::quote(&local_id)
        ));
    }

    for query_field in &all_query_fields {
        match &query_field.set {
            None => (),
            Some(val) => {
                let str = to_sql::render_value(&val);
                insert_values.push(str);
            }
        }
    }

    if has_updated_at_field && !updated_at_explicitly_set {
        insert_values.push("unixepoch()".to_string());
    }

    statements.push(to_sql::ignore(format!(
        "insert into {} ({})\n  select {}\n  from {}",
        table_name,
        field_names.join(", "),
        insert_values.join(", "),
        parent_table_name
    )));

    let temp_table_name = &get_temp_table_name(&query_table_field);

    // Create temp table for nested inserts if tracking affected rows
    // This must happen AFTER the insert to capture the inserted rowids
    if include_affected_rows {
        // Create temp table with rowids of inserted rows by joining on foreign key
        let foreign_key = &link.foreign.fields[0];
        let local_key = &link.local_ids[0];
        let quoted_foreign_key = string::quote(foreign_key);
        let quoted_local_key = string::quote(local_key);
        let quoted_table_name_for_temp = string::quote(&table_name);
        let quoted_parent_table = string::quote(parent_table_name);
        statements.push(to_sql::ignore(format!(
            "create temp table {} as\n  select t.rowid as id\n  from {} t\n  join {} p on t.{} = p.{}",
            temp_table_name,
            quoted_table_name_for_temp,
            quoted_parent_table,
            quoted_foreign_key,
            quoted_local_key
        )));
    }

    for query_field in all_query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                // We are inserting a link, so we need to do a nested insert
                let linked_table = typecheck::get_linked_table(context, &link).unwrap();

                // Track nested table
                if include_affected_rows {
                    let nested_temp_table_name = get_temp_table_name(query_field);
                    let linked_table_name =
                        ast::get_tablename(&linked_table.record.name, &linked_table.record.fields);
                    let linked_column_names =
                        collect_all_column_names(context, &linked_table.record.fields);
                    affected_tables.push(AffectedTable {
                        table_name: linked_table_name,
                        column_names: linked_column_names,
                        temp_table_name: nested_temp_table_name.clone(),
                    });
                }

                insert_linked(
                    indent + 2,
                    context,
                    query,
                    &temp_table_name,
                    linked_table,
                    query_field,
                    &link,
                    statements,
                    include_affected_rows,
                    affected_tables,
                );
            }
            _ => (),
        }
    }
}

fn generate_affected_rows_query_for_inserts(
    _context: &typecheck::Context,
    _query_info: &typecheck::QueryInfo,
    affected_tables: &Vec<AffectedTable>,
) -> String {
    let mut union_parts = Vec::new();

    for affected_table in affected_tables {
        let quoted_table_name = string::quote(&affected_table.table_name);

        // Build json_array call for each row - values in same order as headers
        let mut row_value_parts = Vec::new();
        for col in &affected_table.column_names {
            // Quote both table and column to handle special characters like __
            // Column names with __ are valid unquoted identifiers in SQLite, but we quote them for safety
            row_value_parts.push(format!(
                "{}.{}",
                quoted_table_name,
                string::quote(col)
            ));
        }

        // Build json_array call for headers
        // Headers should just be column names in single quotes (for JSON strings), not double-quoted
        let mut header_parts = Vec::new();
        for col in &affected_table.column_names {
            header_parts.push(format!("'{}'", col));
        }
        // Build the join condition - all tables use their temp table
        // Use table name directly instead of alias to avoid issues with quoted column names
        let join_condition = format!(
            "join {} temp_table on {}.rowid = temp_table.id",
            affected_table.temp_table_name, quoted_table_name
        );

        // Format: { table_name, headers, rows: [[...], [...]] }
        let select_part = format!(
            "select json_object(\n    'table_name', '{}',\n    'headers', json_array({}),\n    'rows', json_group_array(json_array({}))\n  ) as affected_row\n  from {}\n  {}",
            affected_table.table_name,
            header_parts.join(", "),
            row_value_parts.join(", "),
            quoted_table_name,
            join_condition
        );

        union_parts.push(select_part);
    }

    // Use json() to parse the JSON string before grouping, so we get an array of objects, not an array of strings
    format!(
        "select json_group_array(json(affected_row)) as _affectedRows\nfrom (\n  {}\n)",
        union_parts.join("\n  union all\n  ")
    )
}

// Collect all column names including union type variant columns
fn collect_all_column_names(context: &typecheck::Context, fields: &Vec<ast::Field>) -> Vec<String> {
    let mut column_names = Vec::new();
    collect_column_names_recursive(context, fields, None, &mut column_names);
    column_names
}

fn collect_column_names_recursive(
    context: &typecheck::Context,
    fields: &Vec<ast::Field>,
    parent_name: Option<&str>,
    column_names: &mut Vec<String>,
) {
    for field in fields {
        match field {
            ast::Field::Column(column) => {
                match &column.serialization_type {
                    ast::SerializationType::Concrete(_) => {
                        // Regular column
                        // If parent_name is Some, it already includes trailing __ (e.g., "status__")
                        // So we just concatenate, not add another __
                        let column_name = match parent_name {
                            None => column.name.clone(),
                            Some(parent) => format!("{}{}", parent, column.name),
                        };
                        column_names.push(column_name);
                    }
                    ast::SerializationType::FromType(typename) => {
                        // Union type column
                        let base_name = match parent_name {
                            None => column.name.clone(),
                            Some(parent) => format!("{}__{}", parent, column.name),
                        };
                        // Add the discriminator column
                        column_names.push(base_name.clone());

                        // Add variant field columns
                        if let Some((_, type_)) = context.types.get(typename) {
                            if let typecheck::Type::OneOf { variants } = type_ {
                                // Variant fields are stored as {columnName}__{fieldName}
                                // So if base_name is "status", variant fields become "status__reason"
                                let variant_base_name = format!("{}__", base_name);
                                for variant in variants {
                                    if let Some(var_fields) = &variant.fields {
                                        collect_column_names_recursive(
                                            context,
                                            var_fields,
                                            Some(&variant_base_name),
                                            column_names,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// Field names

fn to_fieldnames(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    query_fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);

        result.append(&mut to_table_fieldname(
            2,
            context,
            &table_name,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_table_fieldname(
    indent: usize,
    context: &typecheck::Context,
    table_name: &str,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(_) => {
            let str = query_field.name.to_string();
            return vec![str];
        }
        _ => vec![],
    }
}

// Insert
fn to_field_insert_values(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        match &field.set {
            None => (),
            Some(val) => {
                let str = to_sql::render_value(&val);
                result.push(str);
            }
        }
    }

    result
}
