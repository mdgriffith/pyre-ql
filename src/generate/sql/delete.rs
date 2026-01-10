use crate::ast;
use crate::ext::string;
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

    if include_affected_rows {
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

        // Create temp table with rows that will be deleted
        statements.push(to_sql::ignore(format!(
            "create temp table {} as select * from {} {}",
            temp_table_name, quoted_table_name, where_clause_str
        )));

        // Execute DELETE
        statements.push(to_sql::ignore(sql.clone()));

        // Format affected rows from temp table
        let affected_rows_sql =
            generate_affected_rows_query(context, query_info, table, query_field, &temp_table_name);
        statements.push(to_sql::include(affected_rows_sql));
    } else {
        statements.push(to_sql::ignore(sql));
    }

    statements
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
