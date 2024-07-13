use crate::ast::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn hash_query_interface(query: &Query) -> String {
    let mut hasher = Sha256::new();

    // Hash operation
    hasher.update(format!("{:?}", query.operation));

    // Hash name
    hasher.update(&query.name);

    // Hash args
    for arg in &query.args {
        let type_string = arg.type_.clone().unwrap_or("".to_string());
        hasher.update(&arg.name);
        hasher.update(type_string);
    }

    // Hash fields
    hash_fields(&mut hasher, &query.fields);

    format!("{:x}", hasher.finalize())
}

pub fn hash_query_full(query: &Query) -> String {
    let mut hasher = Sha256::new();

    // Hash operation
    hasher.update(format!("{:?}", query.operation));

    // Hash name
    hasher.update(&query.name);

    // Hash args (excluding Location fields)
    for arg in &query.args {
        let type_string = arg.type_.clone().unwrap_or("".to_string());
        hasher.update(&arg.name);
        hasher.update(type_string);
    }

    // Hash fields (excluding Location fields)
    hash_fields(&mut hasher, &query.fields);

    format!("{:x}", hasher.finalize())
}

fn hash_fields(hasher: &mut Sha256, fields: &[QueryField]) {
    for field in fields {
        hasher.update(&field.name);
        if let Some(alias) = &field.alias {
            hasher.update(alias);
        }
        if let Some(set) = &field.set {
            hash_query_value(hasher, set);
        }
        for directive in &field.directives {
            hasher.update(directive);
        }
        for arg_field in &field.fields {
            match arg_field {
                ArgField::Field(query_field) => hash_fields(hasher, &[query_field.clone()]),
                ArgField::Arg(located_arg) => hash_arg(hasher, &located_arg.arg),
                ArgField::Line { count } => hasher.update(count.to_string()),
            }
        }
    }
}

fn hash_arg(hasher: &mut Sha256, arg: &Arg) {
    match arg {
        Arg::Limit(value) | Arg::Offset(value) => {
            hasher.update("limit_or_offset");
            hash_query_value(hasher, value);
        }
        Arg::OrderBy(direction, field) => {
            hasher.update("order_by");
            hasher.update(direction_to_string(direction));
            hasher.update(field);
        }
        Arg::Where(where_arg) => {
            hasher.update("where");
            hash_where_arg(hasher, where_arg);
        }
    }
}

fn hash_where_arg(hasher: &mut Sha256, where_arg: &WhereArg) {
    match where_arg {
        WhereArg::Column(column, operator, value) => {
            hasher.update(column);
            hasher.update(format!("{:?}", operator));
            hash_query_value(hasher, value);
        }
        WhereArg::And(args) | WhereArg::Or(args) => {
            hasher.update(if matches!(where_arg, WhereArg::And(_)) {
                "and"
            } else {
                "or"
            });
            for arg in args {
                hash_where_arg(hasher, arg);
            }
        }
    }
}

fn hash_query_value(hasher: &mut Sha256, value: &QueryValue) {
    match value {
        QueryValue::Variable((_, var)) => {
            hasher.update("variable");
            hasher.update(var);
        }
        QueryValue::String((_, s)) => {
            hasher.update("string");
            hasher.update(s);
        }
        QueryValue::Int((_, i)) => {
            hasher.update("int");
            hasher.update(&i.to_string());
        }
        QueryValue::Float((_, f)) => {
            hasher.update("float");
            hasher.update(&f.to_string());
        }
        QueryValue::Bool((_, b)) => {
            hasher.update("bool");
            hasher.update(&b.to_string());
        }
        QueryValue::Null(_) => {
            hasher.update("null");
        }
    }
}
