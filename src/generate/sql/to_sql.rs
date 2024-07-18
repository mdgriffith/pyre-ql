use crate::ast;
use crate::ext::string::{decapitalize, quote};

pub fn format_tablename(name: &str) -> String {
    format!("\"{}\"", crate::ext::string::decapitalize(name))
}

pub fn render_value(value: &ast::QueryValue) -> String {
    match value {
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
