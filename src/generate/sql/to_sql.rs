use crate::ast;
use crate::ext::string;
use crate::typecheck;

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
    fieldname: &String,
) -> String {
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
        ast::QueryValue::String((_, s)) => format!("'{}'", s),
        ast::QueryValue::Int((_, i)) => i.to_string(),
        ast::QueryValue::Float((_, f)) => f.to_string(),
        ast::QueryValue::Bool((_, b)) => b.to_string(),
        ast::QueryValue::Null(_) => "null".to_string(),
        ast::QueryValue::LiteralTypeValue((_, details)) => format!("'{}'", details.name),
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
