use crate::ast;
use crate::ext::string;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;

pub fn delete_to_string(
    context: &typecheck::Context,
    _query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    include_affected_rows: bool,
) -> Vec<to_sql::Prepared> {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let mut statements = to_sql::format_attach(query_info);

    // DELETE FROM users
    // WHERE username = 'john_doe';

    let mut sql = format!("delete from {}\n", table_name);

    let mut where_clause = String::new();
    to_sql::render_where(
        context,
        table,
        query_info,
        query_field,
        &ast::QueryOperation::Delete,
        &mut where_clause,
    );
    sql.push_str(&where_clause);

    // SQLite doesn't support DELETE in CTEs or aggregates in RETURNING
    // So we use a temp table approach:
    // 1. SELECT rows that will be deleted into a temp table (before deletion)
    // 2. Execute DELETE
    // 3. Format the results from the temp table
    // Note: Temp tables are automatically cleaned up when the batch's logical connection closes
    // (see docs/sql_remote.md for details). We don't drop them explicitly to avoid lock errors.
    let temp_table_name = format!("_temp_deleted_{}", table_name);
    let quoted_table_name = string::quote(&table_name);

    // Extract WHERE clause from delete SQL to use in SELECT
    let where_clause_str = if let Some(where_pos) = sql.find("where") {
        &sql[where_pos..]
    } else {
        ""
    };

    // Always create temp table - we need it for the typed response query
    statements.push(to_sql::ignore(format!(
        "create temp table {} as select * from {} {}",
        temp_table_name, quoted_table_name, where_clause_str
    )));

    // Execute DELETE
    statements.push(to_sql::ignore(sql));

    // Always generate the typed response query - mutations must return typed data
    let query_field_name = &query_field.name;
    let primary_table_name = select::get_tablename(
        &select::TableAliasKind::Normal,
        table,
        &ast::get_aliased_name(query_field),
    );

    let typed_response_sql = generate_typed_response_query(
        table,
        query_field,
        &primary_table_name,
        &temp_table_name,
    );
    statements.push(to_sql::include(typed_response_sql));

    // Generate affected rows query if requested
    // Execute this BEFORE the final selection to avoid lock conflicts
    if include_affected_rows {
        let affected_rows_sql =
            generate_affected_rows_query(context, query_info, table, query_field, &temp_table_name);
        // Insert before the final selection (which now always exists)
        let final_idx = statements.len() - 1;
        statements.insert(final_idx, to_sql::include(affected_rows_sql));
    }

    statements
}

fn generate_typed_response_query(
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    _primary_table_name: &str,
    temp_table_name: &str,
) -> String {
    let query_field_name = &query_field.name;

    let mut sql = String::new();
    sql.push_str("select\n");
    sql.push_str("  json_object(\n");
    sql.push_str(&format!(
        "    '{}', coalesce(json_group_array(\n      json_object(\n",
        query_field_name
    ));

    // Generate JSON object fields directly from temp table
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
                        ast::Field::Column(_) => {
                            if !first_field {
                                sql.push_str(",\n");
                            }
                            sql.push_str(&format!(
                                "        '{}', {}.{}",
                                aliased_field_name,
                                temp_table_name,
                                string::quote(&query_field.name)
                            ));
                            first_field = false;
                        }
                        _ => continue,
                    }
                }
            }
            _ => continue,
        }
    }

    sql.push_str("\n      )\n    ), json('[]'))\n  ) as ");
    sql.push_str(query_field_name);
    sql.push_str("\nfrom ");
    sql.push_str(temp_table_name);

    sql
}

fn generate_affected_rows_query(
    _context: &typecheck::Context,
    _query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    _query_field: &ast::QueryField,
    temp_table_name: &str,
) -> String {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let columns = ast::collect_columns(&table.record.fields);

    // Generate column names
    let column_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

    // Build json_array call for each row - values in same order as headers
    let mut row_value_parts = Vec::new();
    for col in &column_names {
        let quoted_col = string::quote(col);
        row_value_parts.push(format!("{}.{}", temp_table_name, quoted_col));
    }

    // Build json_array call for headers
    let mut header_parts = Vec::new();
    for col in &column_names {
        header_parts.push(format!("'{}'", col));
    }

    // Format affected rows query - select from temp table and aggregate
    // The temp table contains the rows that were deleted (captured before deletion)
    // Format: { table_name, headers, rows: [[...], [...]] }
    format!(
        "select json_group_array(json(affected_row)) as _affectedRows\nfrom (\n  select json_object(\n    'table_name', '{}',\n    'headers', json_array({}),\n    'rows', json_group_array(json_array({}))\n  ) as affected_row\n  from {}\n)",
        table_name,
        header_parts.join(", "),
        row_value_parts.join(", "),
        temp_table_name
    )
}
