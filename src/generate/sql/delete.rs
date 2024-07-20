use crate::ast;
use crate::ext::string;
use crate::generate::sql::cte;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn delete_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    table: &ast::RecordDetails,
    query_table_field: &ast::QueryField,
) -> String {
    let table_name = ast::get_tablename(&table.name, &table.fields);

    let mut result = format!("delete from {}\n", table_name);
    // DELETE FROM users
    // WHERE username = 'john_doe';

    select::render_where(context, table, query_table_field, &mut result);
    result.push_str(";");

    result
}
