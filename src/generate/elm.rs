use crate::ast;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

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
            let mut result = format!("   {}{}\n{}{{ ", prefix, variant.name, indent);

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_string_field(is_first_field, 6, &field));
                is_first_field = false
            }
            result.push_str("      }\n");
            result
        }
        None => format!("   {}{}\n", prefix, variant.name),
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

    result
        .push_str("module Db.Decode exposing(..)\n\nimport Db\nimport Json.Decode as Decode\n\n\n");

    result.push_str("field : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b\nfield fieldName_ fieldDecoder_ decoder_ =\n    decoder_ |> Decode.andThen (\\func -> Decode.field fieldName_ fieldDecoder_ |> Decode.map func)");

    result.push_str("\n");

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

            result.push_str(&format!(
                "{} : Decode.Decoder Db.{}\n",
                decapitalize(name),
                name
            ));
            result.push_str(&format!("{} =\n", decapitalize(name)));
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

            for field in fields {
                result.push_str(&to_field_decoder(indent_size + 12, &field));
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

fn to_field_decoder(indent: usize, field: &ast::Field) -> String {
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
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}|> Db.Decode.field \"{}\" (Decode.list decode{})\n",
                spaces,
                link.link_name,
                (capitalize(&link.link_name))
            );
        }

        _ => "".to_string(),
    }
}

fn to_type_decoder(type_: &str) -> String {
    match type_ {
        "String" => "Decode.string".to_string(),
        "Int" => "Decode.int".to_string(),
        "Float" => "Decode.float".to_string(),
        _ => format!("Db.Decode.{}", decapitalize(type_)).to_string(),
    }
}

// Encoders!
//

pub fn to_schema_encoders(schem: &ast::Schema) -> String {
    let mut result = String::new();

    result
        .push_str("module Db.Encode exposing(..)\n\nimport Db\nimport Json.Encode as Encode\n\n\n");

    for definition in &schem.definitions {
        result.push_str(&to_encoder_definition(definition));
    }
    result
}

fn to_encoder_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => "".to_string(),
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Tagged { name, variants } => {
            let mut result = "".to_string();

            result.push_str(&format!("encode{} : Db.{} -> Encode.Value\n", name, name));
            result.push_str(&format!("encode{} input_ =\n", name));
            result.push_str("    case input_ of\n");
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_encoder_variant(is_first, 8, name, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { name, fields } => "".to_string(),
    }
}

fn to_encoder_variant(
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
                "{}Db.{} inner_details__ ->\n{}Encode.object\n{}[ ( \"type\", Encode.string \"{}\" )\n",
                outer_indent, variant.name, indent, inner_indent, variant.name
            );

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_field_encoder(is_first_field, indent_size + 8, &field));
                is_first_field = false
            }
            result.push_str(&format!("{}]\n\n", inner_indent));

            result
        }
        None => format!(
            "{}Db.{} ->\n{}Encode.object [ ( \"type\", Encode.string \"{}\" ) ]\n\n",
            outer_indent, variant.name, indent, variant.name
        ),
    }
}

fn to_field_encoder(is_first: bool, indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}, ( \"{}\", {} inner_details__.{})\n",
                spaces,
                column.name,
                to_type_encoder(&column.name, &column.type_),
                column.name
            );
        }

        _ => "".to_string(),
    }
}

fn to_type_encoder(fieldname: &str, type_: &str) -> String {
    match type_ {
        "String" => "Encode.string".to_string(),
        _ => format!("Db.encode{}", type_).to_string(),
    }
}

//  QUERIES
//
pub fn write_queries(context: &typecheck::Context, query_list: &ast::QueryList) -> io::Result<()> {
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let path = &format!("examples/elm/Query/{}.elm", q.name.to_string());
                let target_path = Path::new(path);
                let mut output = fs::File::create(target_path).expect("Failed to create file");
                output
                    .write_all(to_query_file(&context, &q).as_bytes())
                    .expect("Failed to write to file");
            }
            _ => continue,
        }
    }
    Ok(())
}

fn to_query_file(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = format!("module Query.{} exposing (..)\n\n\n", query.name);

    result.push_str("import Db\n");
    result.push_str("import Db.Decode\n");
    result.push_str("import Db.Encode\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Json.Encode as Encode\n");

    result.push_str("\n\n");

    // TODO:: Input types

    //
    // Type Alisaes
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();
        result.push_str(&to_query_type_alias(
            context,
            table,
            &field.name,
            &field.fields,
        ));
    }

    // TODO:: HTTP Sender

    result.push_str("\n\n");
    // TODO:: Decoder
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();
        result.push_str(&to_query_decoder(
            context,
            table,
            &field.name,
            &field.fields,
        ));
    }

    result
}

fn to_query_decoder(
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    name: &str,
    fields: &Vec<ast::QueryField>,
) -> String {
    let mut result = format!(
        "decode{} : Decode.Decoder {}\n",
        capitalize(name),
        capitalize(name)
    );
    result.push_str(&format!("decode{} =\n", capitalize(name)));
    result.push_str(&format!("    Decode.succeed {}\n", capitalize(name)));
    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.push_str(&to_field_decoder(8, &table_field));
    }

    for field in fields {
        if field.fields.is_empty() {
            continue;
        }

        let fieldname_match = table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(f, &field.name));

        match fieldname_match {
            Some(ast::Field::FieldDirective(ast::FieldDirective::Link(link))) => {
                let link_table = context.tables.get(&link.foreign_tablename).unwrap();
                result.push_str("\n\n");
                result.push_str(&to_query_decoder(
                    context,
                    link_table,
                    &field.name,
                    &field.fields,
                ));
            }
            _ => continue,
        }
    }

    result
}

fn to_query_type_alias(
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    name: &str,
    fields: &Vec<ast::QueryField>,
) -> String {
    let mut result = format!("type alias {} =\n", capitalize(name));

    let mut is_first = true;

    for field in fields {
        if is_first {
            result.push_str(&format!("    {{ ",))
        }

        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        match table_field {
            ast::Field::Column(col) => {
                result.push_str(&to_string_query_field(is_first, 4, &field, col));
            }
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                result.push_str(&to_string_query_field_link(is_first, 4, &field, link));
            }
            _ => {}
        }

        if is_first {
            is_first = false;
        }
    }
    result.push_str("    }\n");

    for field in fields {
        if field.fields.is_empty() {
            continue;
        }

        let fieldname_match = table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(f, &field.name));

        match fieldname_match {
            Some(ast::Field::FieldDirective(ast::FieldDirective::Link(link))) => {
                let link_table = context.tables.get(&link.foreign_tablename).unwrap();
                result.push_str("\n\n");
                result.push_str(&to_query_type_alias(
                    context,
                    link_table,
                    &field.name,
                    &field.fields,
                ));
            }
            _ => continue,
        }
    }

    result
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn decapitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
    }
}

fn to_string_query_field_link(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    link_details: &ast::LinkDetails,
) -> String {
    if is_first {
        return format!(
            "{} : {}\n",
            decapitalize(&field.name),
            (format!("List {}", capitalize(&link_details.link_name)))
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}\n",
            spaces,
            decapitalize(&field.name),
            (format!("List {}", capitalize(&link_details.link_name)))
        );
    }
}

fn to_string_query_field(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    table_column: &ast::Column,
) -> String {
    if is_first {
        return format!(
            "{} : {}\n",
            decapitalize(&field.name),
            to_elm_typename(&table_column.type_)
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}\n",
            spaces,
            decapitalize(&field.name),
            to_elm_typename(&table_column.type_)
        );
    }
}

fn to_elm_typename(type_: &str) -> String {
    match type_ {
        "String" => type_.to_string(),
        "Int" => type_.to_string(),
        "Float" => type_.to_string(),
        "Bool" => type_.to_string(),
        _ => format!("Db.{}", type_).to_string(),
    }
}

// Example: ($arg: String)
fn to_string_param_definition(is_first: bool, param: &ast::QueryParamDefinition) -> String {
    if (is_first) {
        format!("{}: {}", param.name, param.type_)
    } else {
        format!(", {}: {}", param.name, param.type_)
    }
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
