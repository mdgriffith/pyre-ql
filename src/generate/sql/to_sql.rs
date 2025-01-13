use crate::ast;
use crate::typecheck;
use crate::ext::string;


pub fn format_tablename(name: &str) -> String {
    format!("\"{}\"", crate::ext::string::decapitalize(name))
}


/// Real meaning it's in the db and we might need to use a schema to target it.
pub fn render_real_field(
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> String {
    if table.schema == query_info.primary_db {
        return format!(
            "{}.{}",
            format_tablename(&table.record.name),
            string::quote(&query_field.name),
        );
    } else {
        return format!(
            "{}.{}.{}",
            string::quote(&table.schema),
            format_tablename(&table.record.name),
            string::quote(&query_field.name),
        );
    };
}

/// Real meaning it's in the db and we might need to use a schema to target it.
pub fn render_real_where_field(
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    fieldname: &String,
) -> String {
    if table.schema == query_info.primary_db {
        return format!(
            "{}.{}",
            format_tablename(&table.record.name),
            string::quote(fieldname),
        );
    } else {
        return format!(
            "{}.{}.{}",
            string::quote(&table.schema),
            format_tablename(&table.record.name),
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
        ast::QueryValue::Variable((r, var)) => {
            format!("${}", var.name)
        }
        ast::QueryValue::String((r, s)) => format!("'{}'", s),
        ast::QueryValue::Int((r, i)) => i.to_string(),
        ast::QueryValue::Float((r, f)) => f.to_string(),
        ast::QueryValue::Bool((r, b)) => b.to_string(),
        ast::QueryValue::Null(r) => "null".to_string(),
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
