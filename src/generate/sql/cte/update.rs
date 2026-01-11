use crate::ast;
use crate::ext::string;
use crate::generate::sql::to_sql;
use crate::typecheck;

pub fn update_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    include_affected_rows: bool,
) -> Vec<to_sql::Prepared> {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);

    let mut statements = to_sql::format_attach(query_info);

    let mut result = String::new();
    result.push_str(&format!("update {}\n", table_name));

    // UPDATE users
    // SET credit = 150
    // WHERE username = 'john_doe';

    let mut values: Vec<String> = Vec::new();

    let all_query_fields = ast::collect_query_fields(&query_field.fields);
    let new_values = &to_field_set_values(table, &all_query_fields);
    values.append(&mut new_values.clone());

    // Check if updatedAt field exists in table and is not explicitly set
    let has_updated_at_field = table
        .record
        .fields
        .iter()
        .any(|f| ast::has_fieldname(f, "updatedAt"));
    let updated_at_explicitly_set = all_query_fields.iter().any(|f| f.name == "updatedAt");

    if has_updated_at_field && !updated_at_explicitly_set {
        values.push("updatedAt = unixepoch()".to_string());
    }

    result.push_str(&format!("set {}", values.join(", ")));

    result.push_str("\n");
    let mut where_clause = String::new();
    to_sql::render_where(
        context,
        table,
        query_info,
        query_field,
        &ast::QueryOperation::Update,
        &mut where_clause,
    );
    result.push_str(&where_clause);

    // Always execute UPDATE (with or without RETURNING)
    if include_affected_rows {
        result.push_str(" returning *");
    }
    statements.push(to_sql::ignore(result));

    // Always generate the typed response query - mutations must return typed data
    let query_field_name = &query_field.name;
    // Use the same table_name as the UPDATE statement for consistency
    let typed_response_sql =
        generate_typed_response_query(table, query_field, &table_name, &where_clause);
    statements.push(to_sql::include(typed_response_sql));

    // Generate affected rows query if requested
    // Execute this BEFORE the final selection to avoid lock conflicts
    if include_affected_rows {
        let affected_rows_sql =
            generate_affected_rows_query(context, query_info, table, query_field, &where_clause);
        // Insert before the final selection (which now always exists)
        let final_idx = statements.len() - 1;
        statements.insert(final_idx, to_sql::include(affected_rows_sql));
    }

    statements
}

fn generate_typed_response_query(
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    table_name: &str,
    where_clause: &str,
) -> String {
    let query_field_name = &query_field.name;
    let quoted_table_name = string::quote(table_name);

    // Replace table name in WHERE clause with alias 't'
    // Use the exact same replacement logic as generate_affected_rows_query
    // quoted_table_name is already "users" (with quotes), so we need to match "users"."column"
    let mut where_with_alias = where_clause.to_string();
    // Pattern 1: "users"."id" -> t."id" (most common)
    where_with_alias = where_with_alias.replace(&format!("{}.\"", quoted_table_name), "t.\"");
    // Pattern 2: users.id -> t.id (unquoted, shouldn't happen but be safe)
    where_with_alias = where_with_alias.replace(&format!("{}.", table_name), "t.");

    let mut sql = String::new();
    sql.push_str("select\n");
    sql.push_str("  coalesce(json_group_array(\n");
    sql.push_str("    json_object(\n");

    // Generate JSON object fields directly from table
    let mut first_field = true;
    for field in &query_field.fields {
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
                        ast::Field::Column(column) => {
                            if !first_field {
                                sql.push_str(",\n");
                            }
                            sql.push_str(&format!("      '{}', ", aliased_field_name));

                            // Handle boolean types: SQLite stores booleans as 0/1, convert to JSON boolean
                            if column.type_ == "Bool" {
                                sql.push_str(&format!(
                                    "json(case when t.{} = 1 then 'true' else 'false' end)",
                                    string::quote(&query_field.name)
                                ));
                            } else {
                                sql.push_str(&format!("t.{}", string::quote(&query_field.name)));
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

    sql.push_str("\n    )\n  ), json('[]')) as ");
    sql.push_str(query_field_name);
    sql.push_str("\nfrom ");
    sql.push_str(&quoted_table_name);
    sql.push_str(" t");
    if !where_with_alias.trim().is_empty() {
        // WHERE clause already includes "where\n" prefix, so just append it
        sql.push_str("\n");
        sql.push_str(&where_with_alias);
    }

    sql
}

fn generate_affected_rows_query(
    _context: &typecheck::Context,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    where_clause: &str,
) -> String {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let columns = ast::collect_columns(&table.record.fields);

    // Generate column names
    let column_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

    // Build json_array call for each row - values in same order as headers
    let mut row_value_parts = Vec::new();
    for col in &column_names {
        let quoted_col = string::quote(col);
        row_value_parts.push(format!("t.{}", quoted_col));
    }

    // Build json_array call for headers
    let mut header_parts = Vec::new();
    for col in &column_names {
        header_parts.push(format!("'{}'", col));
    }

    // Select affected rows using the same WHERE conditions
    // Note: For UPDATE, this selects rows after the update, so the WHERE conditions
    // should still match the updated rows
    // Replace table name in WHERE clause with alias 't'
    // WHERE clauses use format: "users"."id" so we need to replace "users"." with t."
    let quoted_table_name = string::quote(&table_name);
    // Replace quoted table name references: "users"."column" -> t."column"
    // The WHERE clause format is: "where\n "users"."id" = $id\n"
    // We need to replace "users"." with t." to use the alias
    // quoted_table_name is already "users" (with quotes), so we need to match "users"."column"
    let mut where_with_alias = where_clause.to_string();
    // Pattern 1: "users"."id" -> t."id" (most common)
    // quoted_table_name is "users", so we match {quoted_table_name}." which is "users"."
    where_with_alias = where_with_alias.replace(&format!("{}.\"", quoted_table_name), "t.\"");
    // Pattern 2: users.id -> t.id (unquoted, shouldn't happen but be safe)
    where_with_alias = where_with_alias.replace(&format!("{}.", table_name), "t.");
    // Format: { table_name, headers, rows: [[...], [...]] }
    format!(
        "select json_group_array(json(affected_row)) as _affectedRows\nfrom (\n  select json_object(\n    'table_name', '{}',\n    'headers', json_array({}),\n    'rows', json_group_array(json_array({}))\n  ) as affected_row\n  from {} t\n{}\n)",
        table_name,
        header_parts.join(", "),
        row_value_parts.join(", "),
        quoted_table_name,
        where_with_alias.trim()
    )
}

// SET values

fn to_field_set_values(table: &typecheck::Table, fields: &Vec<&ast::QueryField>) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        match &field.set {
            None => (),
            Some(val) => match table_field {
                ast::Field::Column(column) => {
                    let mut str = String::new();

                    str.push_str(&column.name);
                    str.push_str(" = ");
                    str.push_str(&to_sql::render_value(&val));

                    result.push(str);
                }
                _ => (),
            },
        }
    }

    result
}
