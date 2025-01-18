use crate::ast;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;

pub fn delete_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
) -> String {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let mut result = to_sql::format_attach(query_info);
    result.push_str(&format!("delete from {}\n", table_name));
    // DELETE FROM users
    // WHERE username = 'john_doe';

    select::render_where(context, table, query_info, query_field, &mut result);
    result.push_str(";");

    result
}
