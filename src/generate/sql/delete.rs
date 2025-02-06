use crate::ast;
use crate::generate::sql::to_sql;
use crate::typecheck;

pub fn delete_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
) -> Vec<to_sql::Prepared> {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let mut statements = to_sql::format_attach(query_info);

    // DELETE FROM users
    // WHERE username = 'john_doe';

    let mut sql = format!("delete from {}\n", table_name);

    to_sql::render_where(
        context,
        table,
        query_info,
        query_field,
        &ast::QueryOperation::Delete,
        &mut sql,
    );
    statements.push(to_sql::ignore(sql));

    statements
}
