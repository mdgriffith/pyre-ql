use crate::ast;
use crate::ext::string;
use crate::generate::sql::cte;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn update_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    table: &ast::RecordDetails,
    query_table_field: &ast::QueryField,
) -> String {
    let table_name = ast::get_tablename(&table.name, &table.fields);

    let mut result = format!("update {}\n", table_name);

    // UPDATE users
    // SET credit = 150
    // WHERE username = 'john_doe';

    let mut values: Vec<String> = Vec::new();

    let mut new_values = &to_field_set_values(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    values.append(&mut new_values.clone());

    result.push_str(&format!("set {}", values.join(", ")));

    result.push_str("\n");
    select::render_where(context, table, query_table_field, &mut result);
    result.push_str(";");
    result
}

// SET values

fn to_field_set_values(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
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
