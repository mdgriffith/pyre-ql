use crate::ast;
use crate::ext::string;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

//  QUERIES
//
pub fn write_queries(context: &typecheck::Context, query_list: &ast::QueryList) -> io::Result<()> {
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let path = &format!("examples/sql/{}.sql", q.name.to_string());
                let target_path = Path::new(path);
                let mut output = fs::File::create(target_path).expect("Failed to create file");
                output
                    .write_all(to_string(&context, &q).as_bytes())
                    .expect("Failed to write to file");
            }
            _ => continue,
        }
    }
    Ok(())
}

pub fn to_string(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = "select\n".to_string();

    // Selection
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();
        let selected = &to_selection(
            context,
            &ast::get_aliased_name(&field),
            table,
            &ast::collect_query_fields(&field.fields),
        );
        // result.push_str(&format!("  {}", quote(&field.name)));
        result.push_str("  ");
        result.push_str(&selected.join(",\n  "));
        result.push_str("\n");
    }

    // FROM
    result.push_str("from\n");
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();
        let mut from_vals = &to_from(
            context,
            &ast::get_aliased_name(&field),
            table,
            &ast::collect_query_fields(&field.fields),
        );
        result.push_str(&format!("  {}", quote(&field.name)));
        if (from_vals.is_empty()) {
            result.push_str("\n");
        } else {
            result.push_str("\n  ");
        }

        result.push_str(&from_vals.join(",\n  "));
    }

    // WHERE
    let mut where_vals = vec![];
    for query_field in &query.fields {
        let table = context.tables.get(&query_field.name).unwrap();
        let table_alias = &ast::get_aliased_name(&query_field);

        let new_params =
            render_where_params(&ast::collect_query_args(&query_field.fields), table_alias);

        where_vals.extend(new_params);

        let new_where_vals = to_where(
            context,
            table_alias,
            table,
            &ast::collect_query_fields(&query_field.fields),
        );

        where_vals.extend(new_where_vals);
    }
    if (!&where_vals.is_empty()) {
        result.push_str("where\n  ");
        let mut first = true;
        for wher in &where_vals {
            if (first) {
                result.push_str(&format!("{}\n", wher));
                first = false;
            } else {
                result.push_str(&format!(" {}\n", wher));
            }
        }
    }

    // Order by
    let mut order_vals = vec![];
    for query_field in &query.fields {
        let table = context.tables.get(&query_field.name).unwrap();
        let table_alias = &ast::get_aliased_name(&query_field);

        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(ast::Arg::OrderBy(dir, col)) => {
                    let dir_str = ast::direction_to_string(dir);
                    order_vals.push(format!("{}.{} {}", quote(table_alias), quote(col), dir_str));
                }
                _ => continue,
            }
        }
    }
    if (!&order_vals.is_empty()) {
        result.push_str("order by ");

        let mut first = true;

        for (i, order) in order_vals.iter().enumerate() {
            if (first) {
                result.push_str(order);
                first = false;
            } else {
                result.push_str(&format!(", {}", order));
            }
        }
    }

    // LIMIT
    for query_field in &query.fields {
        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(ast::Arg::Limit(val)) => {
                    result.push_str("\n");
                    result.push_str(&format!("limit {}", render_value(val)));
                    break;
                }
                _ => continue,
            }
        }
    }

    // OFFSET
    for query_field in &query.fields {
        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(ast::Arg::Offset(val)) => {
                    result.push_str("\n");
                    result.push_str(&format!("offset {}", render_value(val)));
                    break;
                }
                _ => continue,
            }
        }
    }

    result.push_str(";");

    result
}

fn to_selection(
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

        result.append(&mut to_subselection(
            2,
            context,
            &table.name,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subselection(
    indent: usize,
    context: &typecheck::Context,
    table_name: &str,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            let str = format!(
                "{}.{} as {}",
                format_tablename(table_name),
                quote(&query_field.name),
                quote(&ast::get_select_alias(
                    table_alias,
                    table_field,
                    query_field
                ))
            );
            return vec![str];
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            return to_selection(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );
        }

        _ => vec![],
    }
}

// FROM
//
fn to_from(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.append(&mut to_subfrom(
            2,
            context,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subfrom(
    indent: usize,
    context: &typecheck::Context,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            let mut inner_list = to_from(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );
            let join = format!(
                "left join {} on \"{}\".\"{}\" = {}.\"{}\"\n",
                format_tablename(&link.foreign_tablename),
                table_alias,
                link.local_ids.join(""),
                format_tablename(&link.foreign_tablename),
                link.foreign_ids.join(""),
            );
            inner_list.push(join);
            return inner_list;
        }

        _ => vec![],
    }
}

// WHERE
//
fn to_where(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.append(&mut to_subwhere(
            2,
            context,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subwhere(
    indent: usize,
    context: &typecheck::Context,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            return render_where_params(&ast::collect_query_args(&query_field.fields), table_alias);
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            let mut inner_list = to_where(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );

            return inner_list;
        }

        _ => vec![],
    }
}

fn render_where_params(args: &Vec<&ast::Arg>, table_alias: &str) -> Vec<String> {
    let mut result = vec![];
    for where_arg in ast::collect_where_args(args) {
        result.push(render_where_arg(&where_arg, table_alias));
    }
    result
}

fn render_value(value: &ast::QueryValue) -> String {
    match value {
        ast::QueryValue::Variable(v) => format!("${}", v),
        ast::QueryValue::String(s) => format!("'{}'", s),
        ast::QueryValue::Int(i) => i.to_string(),
        ast::QueryValue::Float(f) => f.to_string(),
        ast::QueryValue::Bool(b) => b.to_string(),
        ast::QueryValue::Null => "null".to_string(),
    }
}

fn render_where_arg(arg: &ast::WhereArg, table_alias: &str) -> String {
    match arg {
        ast::WhereArg::Column(name, operator, value) => {
            let qualified_column_name =
                format!("{}.{}", format_tablename(table_alias), quote(name));
            let operator = match operator {
                ast::Operator::Equal => "=",
                ast::Operator::NotEqual => "!=",
                ast::Operator::GreaterThan => ">",
                ast::Operator::LessThan => "<",
                ast::Operator::GreaterThanOrEqual => ">=",
                ast::Operator::LessThanOrEqual => "<=",
                ast::Operator::In => "in",
                ast::Operator::NotIn => "not in",
                ast::Operator::Like => "like",
                ast::Operator::NotLike => "not like",
            };
            let value = render_value(value);
            format!("{} {} {}", qualified_column_name, operator, value)
        }
        ast::WhereArg::And(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table_alias));
            }
            format!("({})", inner_list.join(" and "))
        }
        ast::WhereArg::Or(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table_alias));
            }
            format!("({})", inner_list.join(" or "))
        }
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s)
}

fn format_tablename(name: &str) -> String {
    format!("\"{}\"", crate::ext::string::decapitalize(name))
}