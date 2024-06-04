use crate::ast;

pub fn schema(schem: &ast::Schema) -> String {
    let mut result = String::new();
    for definition in &schem.definitions {
        result.push_str(&to_string_definition(definition));
    }
    result
}

fn to_string_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Definition::Comment { text } => format!("// {}\n", text),
        ast::Definition::Tagged { name, variants } => {
            let mut result = format!("type {}\n", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_string_variant(is_first, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { name, fields } => {
            let mut result = format!("record {} {{\n", name);
            for field in fields {
                result.push_str(&to_string_field(4, field));
            }
            result.push_str("}\n");
            result
        }
    }
}

fn to_string_variant(is_first: bool, variant: &ast::Variant) -> String {
    let prefix = if is_first { " = " } else { " | " };

    match &variant.data {
        Some(fields) => {
            let mut result = format!("  {}{} {{\n", prefix, variant.name);
            for field in fields {
                result.push_str(&to_string_field(8, field));
            }
            result.push_str("     }\n");
            result
        }
        None => format!("  {}{}\n", prefix, variant.name),
    }
}

fn to_string_field(indent: usize, field: &ast::Field) -> String {
    let spaces = " ".repeat(indent);
    format!("{}{}: {}\n", spaces, field.name, field.type_)
}

//
pub fn query(query_list: &ast::QueryList) -> String {
    let mut result = String::new();
    for operation in &query_list.queries {
        result.push_str(&to_string_query_definition(operation));
    }
    result
}

fn to_string_query_definition(definition: &ast::QueryDef) -> String {
    match definition {
        ast::QueryDef::Query(q) => to_string_query(q),
        ast::QueryDef::QueryComment { text } => format!("// {}\n", text),
        ast::QueryDef::QueryLines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
    }
}

fn to_string_query(query: &ast::Query) -> String {
    let mut result = format!("query {}", query.name);

    if (query.args.len() > 0) {
        result.push_str("(");
    }
    let mut first = true;
    for param in &query.args {
        result.push_str(&to_string_param_definition(first, &param));
        first = false;
    }
    if (query.args.len() > 0) {
        result.push_str(")");
    }

    // Fields
    result.push_str(" {\n");

    for field in &query.fields {
        result.push_str(&to_string_query_field(4, &field));
    }
    result.push_str("}\n");
    result
}

// Example: ($arg: String)
fn to_string_param_definition(is_first: bool, param: &ast::QueryParamDefinition) -> String {
    if (is_first) {
        format!("{}: {}", param.name, param.type_)
    } else {
        format!(", {}: {}", param.name, param.type_)
    }
}

fn to_string_query_field(indent: usize, field: &ast::QueryField) -> String {
    let spaces = " ".repeat(indent);
    let mut result = format!("{}{}", spaces, field.name);

    // Args
    if (field.params.len() > 0) {
        result.push_str("(");
    }
    let mut first = true;
    for param in &field.params {
        result.push_str(&to_string_param(first, &param));
        first = false;
    }
    if (field.params.len() > 0) {
        result.push_str(")");
    }

    // Fields
    if (field.fields.len() > 0) {
        result.push_str(" {\n");
    }
    for inner_field in &field.fields {
        result.push_str(&to_string_query_field(indent + 4, &inner_field));
    }
    if (field.fields.len() > 0) {
        result.push_str(&spaces);
        result.push_str("}");
    }
    result.push_str("\n");
    result
}

// Example: (arg = $id)
fn to_string_param(is_first: bool, param: &ast::QueryParam) -> String {
    let operator = operator_to_string(&param.operator);
    let value = value_to_string(&param.value);

    if (is_first) {
        format!("{} {} {}", param.name, operator, value)
    } else {
        format!(", {} {} {}", param.name, operator, value)
    }
}

fn value_to_string(value: &ast::QueryValue) -> String {
    match value {
        ast::QueryValue::Variable(name) => format!("${}", name),
        ast::QueryValue::String(value) => format!("\"{}\"", value),
        ast::QueryValue::Int(value) => value.to_string(),
        ast::QueryValue::Float(value) => value.to_string(),
        ast::QueryValue::Bool(value) => value.to_string(),
        ast::QueryValue::Null => "null".to_string(),
    }
}

fn operator_to_string(operator: &ast::Operator) -> &str {
    match operator {
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
    }
}
