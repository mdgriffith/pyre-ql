use crate::ast;

pub fn schema_to_string(schem: &ast::Schema) -> String {
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
        ast::Definition::Comment { text } => format!("//{}\n", text),
        ast::Definition::Tagged {
            name,
            variants,
            start,
            end,
        } => {
            let mut result = format!("type {}\n", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_string_variant(is_first, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record {
            name,
            fields,
            start,
            end,
            start_name,
            end_name,
        } => {
            let mut result = format!("record {} {{\n", name);

            for field in fields {
                result.push_str(&to_string_field(4, &field));
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
                result.push_str(&to_string_field(8, &field));
            }
            result.push_str("     }\n");
            result
        }
        None => format!("  {}{}\n", prefix, variant.name),
    }
}

fn to_string_field(indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::ColumnLines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Field::Column(column) => to_string_column(indent, column),
        ast::Field::ColumnComment { text } => format!("{}//{}\n", " ".repeat(indent), text),
        ast::Field::FieldDirective(directive) => to_string_field_directive(indent, directive),
    }
}

fn to_string_column(indent: usize, column: &ast::Column) -> String {
    let spaces = " ".repeat(indent);
    format!(
        "{}{}: {}{}\n",
        spaces,
        column.name,
        column.type_,
        to_string_directives(&column.directives)
    )
}

fn to_string_field_directive(indent: usize, directive: &ast::FieldDirective) -> String {
    let spaces = " ".repeat(indent);
    match directive {
        ast::FieldDirective::Watched(_) => format!("{}@watch\n", spaces),
        ast::FieldDirective::TableName((range, name)) => {
            format!("{}@tablename \"{}\"\n", spaces, name)
        }
        ast::FieldDirective::Link(details) => {
            let mut result = format!("{}@link ", spaces);
            result.push_str(&to_string_link_details(details));
            result.push_str("\n");
            result
        }
    }
}

fn to_string_link_details(details: &ast::LinkDetails) -> String {
    let mut result = format!("{} {{ from: ", details.link_name);

    if (*&details.local_ids.len() > 1) {
        for id in &details.local_ids {
            result.push_str(id);
            result.push_str(", ");
        }
    } else {
        for id in &details.local_ids {
            result.push_str(id);
        }
    }

    result.push_str(", to: ");
    if (*&details.foreign_ids.len() > 1) {
        for id in &details.foreign_ids {
            result.push_str(&details.foreign_tablename);
            result.push_str(".");
            result.push_str(id);
            result.push_str(", ");
        }
    } else {
        for id in &details.foreign_ids {
            result.push_str(&details.foreign_tablename);
            result.push_str(".");
            result.push_str(id);
        }
    }
    result.push_str(" }");

    result
}

fn to_string_directives(directives: &Vec<ast::ColumnDirective>) -> String {
    let mut result = String::new();
    for directive in directives {
        result.push_str(" ");
        result.push_str(&to_string_directive(directive));
    }
    result
}

fn to_string_directive(directive: &ast::ColumnDirective) -> String {
    match directive {
        ast::ColumnDirective::PrimaryKey => "@id".to_string(),
        ast::ColumnDirective::Unique => "@unique".to_string(),
        ast::ColumnDirective::Default(def) => match def {
            ast::DefaultValue::Now => "@default(now)".to_string(),
            ast::DefaultValue::Value(value) => {
                format!("@default({})", &value_to_string(value))
            }
        },
    }
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
        ast::QueryDef::QueryComment { text } => format!("//{}\n", text),
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
    let operation_name = match &query.operation {
        ast::QueryOperation::Select => "query",
        ast::QueryOperation::Insert => "insert",
        ast::QueryOperation::Delete => "delete",
        ast::QueryOperation::Update => "update",
    };
    let mut result = format!("{} {}", operation_name, query.name);

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
        format!("${}: {}", param.name, param.type_)
    } else {
        format!(", ${}: {}", param.name, param.type_)
    }
}

fn to_string_field_arg(indent: usize, field_arg: &ast::ArgField) -> String {
    match field_arg {
        ast::ArgField::Arg(arg) => {
            let spaces = " ".repeat(indent);
            format!("{}{}", spaces, &to_string_param(&arg.arg))
        }
        ast::ArgField::Field(field) => to_string_query_field(indent, field),
        ast::ArgField::Line { count } => "\n".repeat(count.clone()),
    }
}

fn to_string_query_field(indent: usize, field: &ast::QueryField) -> String {
    let spaces = " ".repeat(indent);
    let alias_string = match &field.alias {
        Some(alias) => format!("{}: ", alias),
        None => "".to_string(),
    };

    let mut result = format!("{}{}{}", spaces, alias_string, field.name);

    match &field.set {
        Some(val) => {
            result.push_str(" = ");
            result.push_str(&value_to_string(val));
            result.push_str(" ");
        }
        None => {}
    }

    if (field.fields.len() > 0) {
        result.push_str(" {\n");
    }

    // Fields
    for inner_field in &field.fields {
        result.push_str(&to_string_field_arg(indent + 4, &inner_field));
    }
    if (field.fields.len() > 0) {
        result.push_str(&spaces);
        result.push_str("}");
    }
    result.push_str("\n");
    result
}

// Example: (arg = $id)
fn to_string_param(arg: &ast::Arg) -> String {
    match arg {
        ast::Arg::Limit(lim) => {
            format!("@limit {}\n", value_to_string(lim))
        }
        ast::Arg::Offset(off) => {
            format!("@offset {}\n", value_to_string(off))
        }
        ast::Arg::OrderBy(direction, column) => {
            format!("@sort {} {}\n", column, ast::direction_to_string(direction))
        }
        ast::Arg::Where(where_arg) => format!("@where {}\n", format_where(where_arg)),
    }
}

fn format_where(where_arg: &ast::WhereArg) -> String {
    match where_arg {
        ast::WhereArg::Column(column, operator, value) => {
            let operator = operator_to_string(&operator);
            let value = value_to_string(&value);
            format!("{} {} {}", column, operator, value)
        }
        ast::WhereArg::And(and) => {
            let mut result = String::new();
            let last_index = and.len() - 1;
            for (i, arg) in and.iter().enumerate() {
                result.push_str(&format_where(arg));
                if i != last_index {
                    result.push_str(" && ");
                }
            }
            result
        }
        ast::WhereArg::Or(or) => {
            let mut result = String::new();
            let last_index = or.len() - 1;
            for (i, arg) in or.iter().enumerate() {
                result.push_str(&format_where(arg));
                if i != last_index {
                    result.push_str(" || ");
                }
            }
            result
        }
    }
}

fn value_to_string(value: &ast::QueryValue) -> String {
    match value {
        ast::QueryValue::Variable((r, name)) => format!("${}", name),
        ast::QueryValue::String((r, value)) => format!("\"{}\"", value),
        ast::QueryValue::Int((r, value)) => value.to_string(),
        ast::QueryValue::Float((r, value)) => value.to_string(),
        ast::QueryValue::Bool((r, true)) => "True".to_string(),
        ast::QueryValue::Bool((r, false)) => "False".to_string(),
        ast::QueryValue::Null(r) => "null".to_string(),
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
