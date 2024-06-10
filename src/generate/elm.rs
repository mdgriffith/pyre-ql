use crate::ast;

pub fn schema(schem: &ast::Schema) -> String {
    let mut result = String::new();

    result.push_str("module Db exposing(..)\n\n");

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
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Tagged { name, variants } => {
            let mut result = format!("type {}\n", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_string_variant(is_first, 4, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { name, fields } => to_type_alias(name, fields),
    }
}

fn to_type_alias(name: &str, fields: &Vec<ast::Field>) -> String {
    let mut result = format!("type alias {} =\n", name);

    let mut is_first = true;
    for field in fields {
        if is_first & ast::is_column(field) {
            result.push_str(&format!("    {{ ",))
        }
        if (ast::is_column_space(field)) {
            continue;
        }

        result.push_str(&to_string_field(is_first, 4, &field));

        if is_first & ast::is_column(field) {
            is_first = false;
        }
    }
    result.push_str("    }\n");
    result
}

fn to_string_variant(is_first: bool, indent_size: usize, variant: &ast::Variant) -> String {
    let prefix = if is_first { " = " } else { " | " };

    match &variant.data {
        Some(fields) => {
            let indent = " ".repeat(indent_size + 2);
            let mut result = format!("  {}{}\n{}{{ ", prefix, variant.name, indent);

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_string_field(is_first_field, 6, &field));
                is_first_field = false
            }
            result.push_str("      }\n");
            result
        }
        None => format!("  {}{}\n", prefix, variant.name),
    }
}

fn to_string_field(is_first: bool, indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::ColumnLines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Field::Column(column) => to_string_column(is_first, indent, column),
        ast::Field::ColumnComment { text } => "".to_string(),
        ast::Field::FieldDirective(directive) => "".to_string(),
    }
}

fn to_string_column(is_first: bool, indent: usize, column: &ast::Column) -> String {
    if is_first {
        return format!("{} : {}\n", column.name, column.type_);
    } else {
        let spaces = " ".repeat(indent);
        return format!("{}, {} : {}\n", spaces, column.name, column.type_);
    }
}

// DECODE
//

pub fn to_schema_decoders(schem: &ast::Schema) -> String {
    let mut result = String::new();

    result.push_str("module Db.Decode exposing(..)\n\nimport Db\nimport Json.Decode as Decode\n\n");

    for definition in &schem.definitions {
        result.push_str(&to_decoder_definition(definition));
    }
    result
}

fn to_decoder_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Tagged { name, variants } => {
            let mut result = "".to_string();

            for variant in variants {
                match &variant.data {
                    Some(fields) => {
                        result.push_str(&to_type_alias(
                            &format!("{}_{}", name, variant.name),
                            fields,
                        ));
                        result.push_str("\n\n");
                    }
                    None => continue,
                }
            }

            result.push_str(&format!("decode{} : Decode.Decoder Db.{}\n", name, name));
            result.push_str(&format!("decode{} =\n", name));
            result.push_str("    Decode.field \"type\" Decode.string\n");
            result.push_str("        |> Decode.andThen\n");
            result.push_str("            (\\variant_name ->\n");
            result.push_str("               case variant_name of\n");
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_decoder_variant(is_first, 18, name, variant));
                is_first = false;
            }
            result.push_str("            )\n");
            result
        }
        ast::Definition::Record { name, fields } => "".to_string(),
    }
}

fn to_decoder_variant(
    is_first: bool,
    indent_size: usize,
    typename: &str,
    variant: &ast::Variant,
) -> String {
    let outer_indent = " ".repeat(indent_size);
    let indent = " ".repeat(indent_size + 4);
    let inner_indent = " ".repeat(indent_size + 8);
    match &variant.data {
        Some(fields) => {
            let mut result = format!(
                "{}\"{}\" ->\n{}Decode.map Db.{}\n{}(Decode.succeed {}_{}\n",
                outer_indent,
                variant.name,
                indent,
                variant.name,
                inner_indent,
                typename,
                variant.name
            );

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_field_decoder(is_first_field, indent_size + 12, &field));
                is_first_field = false
            }
            result.push_str(&format!("{})\n\n", inner_indent));

            result
        }
        None => format!(
            "{}\"{}\" ->\n{}Decode.succeed Db.{}\n\n",
            outer_indent, variant.name, indent, variant.name
        ),
    }
}

fn to_field_decoder(is_first: bool, indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}|> Decode.field \"{}\" {}\n",
                spaces,
                column.name,
                to_type_decoder(&column.type_)
            );
        }

        _ => "".to_string(),
    }
}

fn to_type_decoder(type_: &str) -> String {
    match type_ {
        "String" => "Decode.string".to_string(),
        _ => format!("Db.decoder{}", type_).to_string(),
    }
}

//  QUERIES
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
        ast::QueryDef::QueryComment { text } => format!("--{}\n", text),
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
