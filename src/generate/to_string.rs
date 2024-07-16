use crate::ast;
use nom::ToUsize;
use std::collections::BTreeMap;

pub fn schema_to_string(schema_file: &ast::SchemaFile) -> String {
    let mut result = String::new();

    for definition in &schema_file.definitions {
        result.push_str(&to_string_definition(definition));
    }
    result.push_str("\n");
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
        ast::Definition::Session(session) => {
            let mut indent_collection: Indentation = collect_indentation(&session.fields, 4);

            let mut result = "session {{\n".to_string();
            for field in &session.fields {
                result.push_str(&to_string_field(&indent_collection, &field));
            }
            result.push_str("}\n");
            result
        }
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
            let mut indent_collection: Indentation = collect_indentation(&fields, 4);

            let mut result = format!("record {} {{\n", name);
            for field in fields {
                result.push_str(&to_string_field(&indent_collection, &field));
            }
            result.push_str("}\n");
            result
        }
    }
}

#[derive(Debug)]
struct Indentation {
    minimum: usize,
    levels: BTreeMap<usize, FieldIndent>,
}

fn collect_indentation(fields: &Vec<ast::Field>, indent_minimum: usize) -> Indentation {
    let mut indent_collection: BTreeMap<usize, FieldIndent> = BTreeMap::new();
    let mut previous_linenumber: usize = 0;
    for field in fields {
        let maybe_field_indent = get_field_indent(indent_minimum, field);
        match maybe_field_indent {
            Some(indent) => match indent_collection.get(&previous_linenumber) {
                Some(previous_indent) => {
                    if previous_indent.line_end + 1 == indent.line_start {
                        let merged = merge_indents(previous_indent, &indent);

                        indent_collection.insert(previous_linenumber, merged);
                    } else {
                        indent_collection.insert(indent.line_start, indent.clone());
                        previous_linenumber = indent.line_start.clone();
                    }
                }
                None => {
                    indent_collection.insert(indent.line_start, indent.clone());
                    previous_linenumber = indent.line_start.clone();
                }
            },
            None => {
                previous_linenumber = 0;
            }
        }
    }
    Indentation {
        minimum: indent_minimum,
        levels: indent_collection,
    }
}

fn merge_indents(previous_indent: &FieldIndent, indent: &FieldIndent) -> FieldIndent {
    FieldIndent {
        line_start: previous_indent.line_start,
        line_end: indent.line_end,
        name_column: std::cmp::max(previous_indent.name_column, indent.name_column),
        type_column: std::cmp::max(previous_indent.type_column, indent.type_column),
        directive_column: std::cmp::max(previous_indent.directive_column, indent.directive_column),
    }
}

#[derive(Clone, Debug)]
struct FieldIndent {
    line_start: usize,
    line_end: usize,
    name_column: usize,
    type_column: usize,
    directive_column: usize,
}

fn get_field_indent(indent_minimum: usize, field: &ast::Field) -> Option<(FieldIndent)> {
    match field {
        ast::Field::Column(column) => {
            match (&column.start_name, &column.end_name, &column.end_typename) {
                (Some(name_loc), Some(name_end_loc), Some(end_typename)) => {
                    let apply_offset = |column: usize| -> usize {
                        if indent_minimum > name_loc.column {
                            indent_minimum - name_loc.column
                        } else {
                            if column == 0 {
                                return 0;
                            } else {
                                column - 1
                            }
                        }
                    };

                    let nullable_space = if column.nullable { 1 } else { 0 };

                    return Some(FieldIndent {
                        line_start: name_loc.line.to_usize(),
                        line_end: end_typename.line.to_usize(),
                        name_column: apply_offset(name_loc.column),
                        type_column: apply_offset(name_end_loc.column + 2),
                        directive_column: apply_offset(end_typename.column + 1 + nullable_space),
                    });
                }
                _ => (),
            }
        }
        _ => (),
    }

    None
}

fn to_string_variant(is_first: bool, variant: &ast::Variant) -> String {
    let prefix = if is_first { " = " } else { " | " };

    match &variant.data {
        Some(fields) => {
            let mut result = format!("  {}{} {{\n", prefix, variant.name);
            let mut indent_collection: Indentation = collect_indentation(&fields, 8);
            for field in fields {
                result.push_str(&to_string_field(&indent_collection, &field));
            }
            result.push_str("     }\n");
            result
        }
        None => format!("  {}{}\n", prefix, variant.name),
    }
}

fn to_string_field(indent: &Indentation, field: &ast::Field) -> String {
    match field {
        ast::Field::ColumnLines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Field::Column(column) => to_string_column(indent, column),
        ast::Field::ColumnComment { text } => {
            format!("{}//{}\n", " ".repeat(indent.minimum), text)
        }
        ast::Field::FieldDirective(directive) => to_string_field_directive(indent, directive),
    }
}

fn to_string_column(indentation: &Indentation, column: &ast::Column) -> String {
    let initial_indent = " ".repeat(indentation.minimum);
    let nullable = if column.nullable { "?" } else { "" };

    let mut type_indent_len = 1;
    let mut directive_indent_len = 0;

    let line_number: usize = match &column.start_name {
        Some(loc) => loc.line.to_usize(),
        None => 0,
    };

    let maybe_indent = indentation
        .levels
        .range(..=line_number)
        .next_back()
        .map(|(_, v)| v);

    match maybe_indent {
        Some(indent) => {
            let name_plus_colon = indentation.minimum + 1 + column.name.len();

            if name_plus_colon < indent.type_column {
                type_indent_len = indent.type_column - name_plus_colon;
            }

            let name_plus_colon_plus_type =
                name_plus_colon + type_indent_len + column.type_.len() + nullable.len() + 1;
            if name_plus_colon_plus_type < indent.directive_column && column.directives.len() > 0 {
                directive_indent_len = indent.directive_column - name_plus_colon_plus_type;
            }
        }
        None => (),
    }

    format!(
        "{}{}:{}{}{}{}{}\n",
        initial_indent,
        column.name,
        " ".repeat(type_indent_len),
        column.type_,
        nullable,
        " ".repeat(directive_indent_len),
        to_string_directives(&column.directives)
    )
}

fn to_string_field_directive(indent: &Indentation, directive: &ast::FieldDirective) -> String {
    let spaces = " ".repeat(indent.minimum);
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
        match &param.type_ {
            None => return format!("${}", param.name),
            Some(type_) => return format!("${}: {}", param.name, type_),
        }
    } else {
        match &param.type_ {
            None => return format!(", ${}", param.name),
            Some(type_) => return format!(", ${}: {}", param.name, type_),
        }
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
        ast::QueryValue::Variable((r, var)) => ast::to_pyre_variable_name(var),
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
