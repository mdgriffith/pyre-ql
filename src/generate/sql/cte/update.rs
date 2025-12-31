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

    if include_affected_rows {
        // Execute UPDATE with RETURNING
        result.push_str(" returning *");
        statements.push(to_sql::ignore(result.clone()));

        // Generate the affected rows query - select rows matching the WHERE conditions
        // Note: For UPDATE, this selects the updated rows (after update)
        let affected_rows_sql =
            generate_affected_rows_query(context, query_info, table, query_field, &where_clause);
        statements.push(to_sql::include(affected_rows_sql));
    } else {
        statements.push(to_sql::ignore(result));
    }

    statements
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

    // Generate column names and json_object keys
    let column_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

    // Build json_object call for row data
    let mut row_parts = Vec::new();
    for col in &column_names {
        let quoted_col = string::quote(col);
        row_parts.push(format!("'{}', t.{}", quoted_col, quoted_col));
    }

    // Build json_array call for headers
    let mut header_parts = Vec::new();
    for col in &column_names {
        header_parts.push(format!("'{}'", string::quote(col)));
    }

    // Select affected rows using the same WHERE conditions
    // Note: For UPDATE, this selects rows after the update, so the WHERE conditions
    // should still match the updated rows
    // Replace table name in WHERE clause with alias 't'
    let quoted_table_name = string::quote(&table_name);
    let where_with_alias = where_clause.replace(&format!("{}.", quoted_table_name), "t.");
    // Use json() to parse the JSON string before grouping, so we get an array of objects, not an array of strings
    format!(
        "select json_group_array(json(affected_row)) as _affectedRows\nfrom (\n  select json_object(\n    'table_name', '{}',\n    'row', json_object({}),\n    'headers', json_array({})\n  ) as affected_row\n  from {} t\n{}\n)",
        table_name,
        row_parts.join(", "),
        header_parts.join(", "),
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
