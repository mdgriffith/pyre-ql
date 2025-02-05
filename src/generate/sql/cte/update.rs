use crate::ast;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;

pub fn update_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
) -> Vec<to_sql::Prepared> {
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);

    let mut statements = to_sql::format_attach(query_info);

    let mut result = String::new();
    result.push_str(&format!("update {}\n", table_name));

    // UPDATE users
    // SET credit = 150
    // WHERE username = 'john_doe';

    let mut values: Vec<String> = Vec::new();

    let new_values = &to_field_set_values(
        context,
        &ast::get_aliased_name(&query_field),
        table,
        &ast::collect_query_fields(&query_field.fields),
    );
    values.append(&mut new_values.clone());

    result.push_str(&format!("set {}", values.join(", ")));

    result.push_str("\n");
    to_sql::render_where(context, table, query_info, query_field, &mut result);

    statements.push(to_sql::include(result));
    statements
}

// SET values

fn to_field_set_values(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
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
