use crate::ast;
use crate::ext::string;
use crate::typecheck;
use serde::Serialize;

// Serializes in a format that libsql can use
#[derive(Serialize)]
#[serde(untagged)]
pub enum SqlAndParams {
    Sql(String),
    SqlWithParams { sql: String, args: Vec<String> },
}

pub struct Prepared {
    pub include: bool,
    pub sql: String,
}

pub fn include(sql: String) -> Prepared {
    Prepared { sql, include: true }
}

pub fn ignore(sql: String) -> Prepared {
    Prepared {
        sql,
        include: false,
    }
}

pub fn format_tablename(name: &str) -> String {
    format!("\"{}\"", crate::ext::string::decapitalize(name))
}

pub fn format_attach(info: &typecheck::QueryInfo) -> Vec<Prepared> {
    let mut attached = vec![];
    if info.attached_dbs.is_empty() {
        return attached;
    }

    for name in info.attached_dbs.iter() {
        attached.push(Prepared {
            include: false,
            sql: format!("attach $db_{} as {}", name, name),
        })
    }

    attached
}

/// Real meaning it's in the db and we might need to use a schema to target it.
pub fn render_real_field(
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> String {
    let table_name = string::quote(&ast::get_tablename(
        &table.record.name,
        &table.record.fields,
    ));

    if table.schema == query_info.primary_db {
        return format!("{}.{}", table_name, string::quote(&query_field.name),);
    } else {
        return format!(
            "{}.{}.{}",
            string::quote(&table.schema),
            table_name,
            string::quote(&query_field.name),
        );
    };
}

/// Real meaning it's in the db and we might need to use a schema to target it.
pub fn render_real_where_field(
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    is_session_var: bool,
    fieldname: &String,
) -> String {
    // Check if this is a Session variable (e.g., Session.userId, Session.role)
    if is_session_var {
        // Session variables are rendered as parameters: userId -> $session_userId
        return format!("$session_{}", fieldname);
    }

    let table_name = string::quote(&ast::get_tablename(
        &table.record.name,
        &table.record.fields,
    ));
    if table.schema == query_info.primary_db {
        return format!("{}.{}", table_name, string::quote(fieldname),);
    } else {
        return format!(
            "{}.{}.{}",
            string::quote(&table.schema),
            table_name,
            string::quote(fieldname),
        );
    };
}

pub fn render_value(value: &ast::QueryValue) -> String {
    match value {
        ast::QueryValue::Fn(func) => format!(
            "{}({})",
            func.name,
            func.args
                .iter()
                .map(|value| render_value(value))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        ast::QueryValue::Variable((_, var)) => {
            format!("${}", var.name)
        }
        ast::QueryValue::String((_, s)) => {
            // Escape single quotes by doubling them (SQL standard)
            let escaped = s.replace("'", "''");
            format!("'{}'", escaped)
        }
        ast::QueryValue::Int((_, i)) => i.to_string(),
        ast::QueryValue::Float((_, f)) => f.to_string(),
        ast::QueryValue::Bool((_, b)) => b.to_string(),
        ast::QueryValue::Null(_) => "null".to_string(),
        ast::QueryValue::LiteralTypeValue((_, details)) => {
            // Escape single quotes by doubling them (SQL standard)
            let escaped = details.name.replace("'", "''");
            format!("'{}'", escaped)
        }
    }
}

pub fn operator(op: &ast::Operator) -> String {
    match op {
        ast::Operator::Equal => "=".to_string(),
        ast::Operator::NotEqual => "!=".to_string(),
        ast::Operator::GreaterThan => ">".to_string(),
        ast::Operator::LessThan => "<".to_string(),
        ast::Operator::GreaterThanOrEqual => ">=".to_string(),
        ast::Operator::LessThanOrEqual => "<=".to_string(),
        ast::Operator::In => "in".to_string(),
        ast::Operator::NotIn => "not in".to_string(),
        ast::Operator::Like => "like".to_string(),
        ast::Operator::NotLike => "not like".to_string(),
    }
}

// WHERE

pub fn render_where(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
    operation: &ast::QueryOperation,
    result: &mut String,
) {
    // Normal @where
    let mut wheres = ast::collect_wheres(&query_field.fields);
    // Add any permissions from the table
    match ast::get_permissions(&table.record, operation) {
        Some(perms) => {
            wheres.push(perms);
        }
        None => {}
    }

    if wheres.is_empty() {
        return;
    }
    result.push_str("where\n");

    // Combine multiple WHERE clauses with AND
    if wheres.len() == 1 {
        let where_str = render_where_arg(&wheres[0], table, query_info, query_field);
        result.push_str(&format!(" {}\n", where_str));
    } else {
        // Multiple WHERE clauses need to be combined with AND
        let combined = ast::WhereArg::And(wheres.clone());
        let where_str = render_where_arg(&combined, table, query_info, query_field);
        result.push_str(&format!(" {}\n", where_str));
    }
}

fn to_where(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_fields: &Vec<&ast::QueryField>,
    operation: &ast::QueryOperation,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for query_field in query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        result.append(&mut to_subwhere(
            context,
            table,
            table_field,
            operation,
            query_info,
            query_field,
        ));
    }

    result
}

fn to_subwhere(
    context: &typecheck::Context,
    table: &typecheck::Table,
    table_field: &ast::Field,
    operation: &ast::QueryOperation,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(_) => {
            let mut wheres = ast::collect_wheres(&query_field.fields);

            // Add any permissions from the table
            match ast::get_permissions(&table.record, operation) {
                Some(perms) => {
                    wheres.push(perms);
                }
                None => {}
            }
            let mut result = vec![];

            for where_arg in &wheres {
                result.push(render_where_arg(&where_arg, table, query_info, query_field));
            }
            return result;
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let link_table = typecheck::get_linked_table(context, &link).unwrap();

            let inner_list = to_where(
                context,
                link_table,
                query_info,
                &ast::collect_query_fields(&query_field.fields),
                operation,
            );

            return inner_list;
        }

        _ => vec![],
    }
}

pub fn render_where_arg(
    arg: &ast::WhereArg,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> String {
    match arg {
        ast::WhereArg::Column(is_session_var, fieldname, op, value) => {
            let qualified_column_name =
                render_real_where_field(table, query_info, *is_session_var, fieldname);

            let operator = operator(op);

            let value = render_value(value);
            format!("{} {} {}", qualified_column_name, operator, value)
        }
        ast::WhereArg::And(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table, query_info, query_field));
            }
            format!("({})", inner_list.join(" and "))
        }
        ast::WhereArg::Or(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table, query_info, query_field));
            }
            format!("({})", inner_list.join(" or "))
        }
    }
}

pub fn render_order_by(
    table: Option<&typecheck::Table>,
    query_info: Option<&typecheck::QueryInfo>,
    query_field: &ast::QueryField,
    result: &mut String,
) {
    let mut order_vals = vec![];

    for field in &query_field.fields {
        match field {
            ast::ArgField::Arg(located_arg) => {
                if let ast::Arg::OrderBy(dir, col) = &located_arg.arg {
                    let order_direction = ast::direction_to_string(dir);
                    let column_ref = if let (Some(table), Some(query_info)) = (table, query_info) {
                        // Use the actual table name with proper schema qualification
                        render_real_where_field(table, query_info, false, col)
                    } else {
                        // Fallback to query field alias (for backward compatibility)
                        let table_alias = &ast::get_aliased_name(&query_field);
                        format!("{}.{}", string::quote(table_alias), string::quote(col))
                    };
                    order_vals.push(format!("{} {}", column_ref, order_direction));
                }
            }
            _ => continue,
        }
    }

    if !&order_vals.is_empty() {
        result.push_str("order by ");

        let mut first = true;

        for order in order_vals.iter() {
            if first {
                result.push_str(order);
                first = false;
            } else {
                result.push_str(&format!(", {}", order));
            }
        }
        result.push_str("\n");
    }
}

// LIMIT

pub fn render_limit(query_field: &ast::QueryField, result: &mut String) {
    for field in &query_field.fields {
        match field {
            ast::ArgField::Arg(located_arg) => {
                if let ast::Arg::Limit(val) = &located_arg.arg {
                    result.push_str(&format!("limit {}\n", render_value(val)));
                    break;
                }
            }
            _ => continue,
        }
    }
}
