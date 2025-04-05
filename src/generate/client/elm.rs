use crate::ast;
use crate::ext::string;
use crate::filesystem::{generate_text_file, GeneratedFile};

use crate::generate::typealias;
use crate::typecheck;
use std::path::Path;
use std::path::PathBuf;

mod rectangle;

const ELM_DECODE_HELP: &str = include_str!("./static/elm/src/Json/Decode/Help.elm");

pub fn generate(database: &ast::Database, files: &mut Vec<GeneratedFile<String>>) {
    files.push(generate_text_file("Db.elm", write_schema(database)));
    files.push(generate_text_file(
        "Json/Decode/Help.elm",
        ELM_DECODE_HELP.to_string(),
    ));
    files.push(generate_text_file(
        "Db/Decode.elm",
        to_schema_decoders(database),
    ));
    files.push(generate_text_file(
        "Db/Encode.elm",
        to_schema_encoders(database),
    ));
}

pub fn write_schema(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("module Db exposing (..)\n\nimport Time\n\n\n");

    result.push_str("type alias DateTime =\n    Time.Posix\n\n\n");

    for schema in &database.schemas {
        for file in &schema.files {
            for definition in &file.definitions {
                result.push_str(&to_string_definition(definition));
            }
        }
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
        ast::Definition::Session(_) => "".to_string(),
        ast::Definition::Comment { .. } => "".to_string(),
        ast::Definition::Tagged { name, variants, .. } => {
            let mut result = format!("type {}\n", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_string_variant(is_first, 4, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { name, fields, .. } => to_type_alias(name, fields),
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
    result.push_str("    }\n\n");
    result
}

fn to_string_variant(is_first: bool, indent_size: usize, variant: &ast::Variant) -> String {
    let prefix = if is_first { " = " } else { " | " };

    match &variant.fields {
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
        ast::Field::ColumnComment { .. } => "".to_string(),
        ast::Field::FieldDirective(_) => "".to_string(),
    }
}

fn to_string_column(is_first: bool, indent: usize, column: &ast::Column) -> String {
    let maybe = if column.nullable { "Maybe " } else { "" };
    if is_first {
        return format!("{} : {}{}\n", column.name, maybe, column.type_);
    } else {
        let spaces = " ".repeat(indent);
        return format!("{}, {} : {}{}\n", spaces, column.name, maybe, column.type_);
    }
}

// DECODE

pub fn to_schema_decoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("module Db.Decode exposing (..)\n\n");
    result.push_str("import Db\n");
    result.push_str("import Db.Read\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Time\n\n\n");

    result.push_str(
        r#"field : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
field fieldName_ fieldDecoder_ decoder_ =
    decoder_ |> Decode.andThen (\func -> Decode.field fieldName_ fieldDecoder_ |> Decode.map func)


bool : Decode.Decoder Bool
bool =
    Decode.oneOf
        [ Decode.bool
        , Decode.int
            |> Decode.andThen
                (\int ->
                    case int of
                        0 ->
                            Decode.succeed False

                        _ ->
                            Decode.succeed True
                )
                ]


dateTime : Decode.Decoder Time.Posix
dateTime =
    Decode.map Time.millisToPosix Decode.int

"#,
    );

    result.push_str("\n\n");

    for schema in &database.schemas {
        for file in &schema.files {
            for definition in &file.definitions {
                to_decoder_definition(definition, &mut result);
            }
        }
    }
    result
}

fn to_decoder_definition(definition: &ast::Definition, result: &mut String) {
    match definition {
        ast::Definition::Lines { count } => (),
        ast::Definition::Session(_) => (),
        ast::Definition::Comment { .. } => (),
        ast::Definition::Tagged { name, variants, .. } => {
            for variant in variants {
                match &variant.fields {
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
                "{} : Db.Read.Decoder Db.{}\n",
                crate::ext::string::decapitalize(name),
                name
            ));
            result.push_str(&format!("{} =\n", crate::ext::string::decapitalize(name)));
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
            result.push_str("        |> Db.Read.custom\n");
        }
        ast::Definition::Record { .. } => (),
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
    match &variant.fields {
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
                result.push_str(&to_variant_field_json_decoder(indent_size + 12, &field));
            }
            result.push_str(&format!("{})\n\n", inner_indent));

            result
        }
        None => format!(
            "{}\"{}\" ->\n{}Decode.succeed Db.{} {}\n\n",
            outer_indent, variant.name, indent, variant.name, "[]"
        ),
    }
}

// Field directives(specifically @link) is not allowed within a type at the moment
fn to_variant_field_json_decoder(indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}|> field \"{}\" {}\n",
                spaces,
                column.name,
                to_json_type_decoder(&column.type_)
            );
        }
        _ => "".to_string(),
    }
}

fn to_json_type_decoder(type_: &str) -> String {
    match type_ {
        "String" => "Decode.string".to_string(),
        "Int" => "Decode.int".to_string(),
        "Float" => "Decode.float".to_string(),
        "DateTime" => "Db.Read.dateTime".to_string(),
        _ => crate::ext::string::decapitalize(type_).to_string(),
    }
}

// Encoders!
//
pub fn to_schema_encoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str(
        "module Db.Encode exposing (..)\n\nimport Db\nimport Json.Encode as Encode\nimport Time\n\n\n",
    );

    result.push_str("dateTime : Time.Posix -> Encode.Value\n");
    result.push_str("dateTime time =\n");
    result.push_str("    Encode.int (Time.posixToMillis time)\n\n");

    for schema in &database.schemas {
        for file in &schema.files {
            for definition in &file.definitions {
                result.push_str(&to_encoder_definition(definition));
            }
        }
    }
    result
}

fn to_encoder_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => "".to_string(),
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Session(_) => "".to_string(),
        ast::Definition::Tagged { name, variants, .. } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} : Db.{} -> Encode.Value\n",
                string::decapitalize(name),
                name
            ));
            result.push_str(&format!("{} input_ =\n", string::decapitalize(name)));
            result.push_str("    case input_ of\n");
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_encoder_variant(is_first, 8, name, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { .. } => "".to_string(),
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
    match &variant.fields {
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
        "Int" => "Encode.int".to_string(),
        "Float" => "Encode.float".to_string(),
        "DateTime" => "Db.Encode.dateTime".to_string(),
        _ => format!("Db.Encode.{}", string::decapitalize(type_)).to_string(),
    }
}

//  QUERIES
//

pub fn generate_queries(
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    base_out_dir: &Path,
    files: &mut Vec<GeneratedFile<String>>,
) {
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                files.push(generate_text_file(
                    base_out_dir
                        .join("Query")
                        .join(format!("{}.elm", q.name.to_string())),
                    to_query_file(context, q),
                ));
            }
            ast::QueryDef::QueryComment { .. } | ast::QueryDef::QueryLines { .. } => continue,
        }
    }
}

fn to_query_file(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = format!("module Query.{} exposing (..)\n\n\n", query.name);

    result.push_str("import Db\n");
    result.push_str("import Db.Decode\n");
    result.push_str("import Db.Encode\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Josn.Decode.Help as Decode\n");
    result.push_str("import Json.Encode as Encode\n");
    result.push_str("import Time\n");
    result.push_str("\n\n");

    result.push_str(&to_param_type_alias(&query.args));

    result.push_str(
        "prepare : Input -> { args : Encode.Value, query : String, decoder : Decode.Decoder ReturnData }\n"
    );
    result.push_str("prepare input =\n");
    result.push_str(&format!(
        "    {{ args = encode input\n    , query = \"{}\"\n    , decoder = decodeReturnResult\n    }}\n\n\n",
        &query.interface_hash,
    ));
    let formatter = typealias::TypeFormatter {
        to_comment: Box::new(|s| format!("{{-| {} -}}\n", s)),
        to_type_def_start: Box::new(|name| format!("type alias {} =\n    {{ ", name)),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
             }| {
                let base_type = to_elm_typename(type_, is_link);

                let type_str = if is_link {
                    if is_optional {
                        format!("Maybe {}", base_type)
                    } else {
                        format!("List {}", base_type)
                    }
                } else {
                    if is_optional {
                        format!("Maybe {}", base_type)
                    } else {
                        base_type.to_string()
                    }
                };
                format!("{} : {}\n", name, type_str)
            },
        ),
        to_type_def_end: Box::new(|| "    }\n".to_string()),
        to_field_separator: Box::new(|is_last| {
            if is_last {
                "".to_string()
            } else {
                "    , ".to_string()
            }
        }),
    };

    // Type Alisaes
    typealias::return_data_aliases(context, query, &mut result, &formatter);

    // Helpers

    let decoder_formatter = typealias::TypeFormatter {
        to_comment: Box::new(|s| format!("{{-| {} -}}\n", s)),
        to_type_def_start: Box::new(|name| {
            format!(
                "decode{} : Decoder {}\ndecode{} =\n    Decode.succeed {}\n        ",
                name, name, name, name
            )
        }),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
             }| {
                let decoder = match type_ {
                    "String" => "Decode.string".to_string(),
                    "Int" => "Decode.int".to_string(),
                    "Float" => "Decode.float".to_string(),
                    "DateTime" => "Db.Decode.dateTime".to_string(),
                    "Boolean" => "Db.Decode.bool".to_string(),
                    _ => {
                        if is_link {
                            format!("decode{}", &type_).to_string()
                        } else {
                            format!("Db.Decode.{}", crate::ext::string::decapitalize(&type_))
                                .to_string()
                        }
                    }
                };

                let final_decoder: String = if is_optional {
                    format!("(Decode.nullable {})", decoder)
                } else {
                    if is_link {
                        format!("(Decode.list {})", decoder)
                    } else {
                        decoder.to_string()
                    }
                };

                format!("|> Decode.andField \"{}\" {}\n", name, final_decoder)
            },
        ),
        to_type_def_end: Box::new(|| "\n".to_string()),
        to_field_separator: Box::new(|_| "        ".to_string()),
    };

    result.push_str(&to_param_type_encoder(&query.args));

    // Top level query decoder

    typealias::return_data_aliases(context, query, &mut result, &decoder_formatter);

    result
}

fn to_param_type_alias(args: &Vec<ast::QueryParamDefinition>) -> String {
    let mut result = "type alias Input =\n".to_string();
    result.push_str("    {");
    let mut is_first = true;
    for arg in args {
        let type_string = &arg.type_.clone().unwrap_or("unknown".to_string());
        if is_first {
            result.push_str(&format!(" {} : {}\n", arg.name, type_string));
            is_first = false;
        } else {
            result.push_str(&format!("    , {} : {}\n", arg.name, type_string));
        }
    }
    result.push_str("    }\n\n\n");
    result
}

fn to_param_type_encoder(args: &Vec<ast::QueryParamDefinition>) -> String {
    let mut result = "encode : Input -> Encode.Value\n".to_string();
    result.push_str("encode input =\n");
    result.push_str("    Encode.object");

    if args.len() == 0 {
        result.push_str(" []\n\n\n");
        return result;
    } else {
        result.push_str("\n");
    }
    let mut is_first = true;
    for arg in args {
        let type_string = &arg.type_.clone().unwrap_or("unknown".to_string());
        if is_first {
            result.push_str(&format!(
                "        [ ( {}, {} input.{} )\n",
                string::quote(&arg.name),
                to_type_encoder(&type_string, &type_string),
                &arg.name
            ));
            is_first = false;
        } else {
            result.push_str(&format!(
                "        , ( {}, {} input.{})\n",
                string::quote(&arg.name),
                to_type_encoder(&type_string, &type_string),
                &arg.name
            ));
        }
    }
    result.push_str("        ]\n\n\n");
    result
}

fn to_elm_typename(type_: &str, is_link: bool) -> String {
    match type_ {
        "String" => type_.to_string(),
        "Int" => type_.to_string(),
        "Float" => type_.to_string(),
        "Bool" => type_.to_string(),
        "DateTime" => "Time.Posix".to_string(),
        _ => {
            if is_link {
                type_.to_string()
            } else {
                format!("Db.{}", type_).to_string()
            }
        }
    }
}
