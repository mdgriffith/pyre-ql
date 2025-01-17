use crate::ast;
use crate::ext::string;
use crate::filesystem;

use crate::typecheck;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

const ELM_READ_MODULE: &str = include_str!("../../static/elm/src/Db/Read.elm");
pub fn write(elm_path: &Path, database: &ast::Database) -> io::Result<()> {
    filesystem::create_dir_if_not_exists(&elm_path)?;
    filesystem::create_dir_if_not_exists(&elm_path.join("Db"))?;

    let formatted_elm = write_schema(database);

    // Top level Elm files
    let elm_db_path = elm_path.join("Db.elm");
    let elm_file = Path::new(&elm_db_path);
    let mut output = fs::File::create(elm_file).expect("Failed to create file");
    output
        .write_all(formatted_elm.as_bytes())
        .expect("Failed to write to file");

    // Decode Helper file
    let elm_db_read_path = elm_path.join("Db/Read.elm");
    let elm_read_file = Path::new(&elm_db_read_path);
    let mut output = fs::File::create(elm_read_file).expect("Failed to create file");
    output
        .write_all(ELM_READ_MODULE.as_bytes())
        .expect("Failed to write to file");

    // Elm Decoders
    let elm_db_decode_path = elm_path.join("Db/Decode.elm");
    let elm_decoders = to_schema_decoders(database);
    let elm_decoder_file = Path::new(&elm_db_decode_path);
    let mut output = fs::File::create(elm_decoder_file).expect("Failed to create file");
    output
        .write_all(elm_decoders.as_bytes())
        .expect("Failed to write to file");

    // Elm Encoders
    let elm_db_encode_path = elm_path.join("Db/Encode.elm");
    let elm_encoders = to_schema_encoders(database);
    let elm_encoder_file = Path::new(&elm_db_encode_path);
    let mut output = fs::File::create(elm_encoder_file).expect("Failed to create file");
    output
        .write_all(elm_encoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
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

const DECODE_BOOL: &str = r#"bool : Decode.Decoder Bool
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
                ]"#;
pub fn to_schema_decoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("module Db.Decode exposing (..)\n\n");
    result.push_str("import Db\n");
    result.push_str("import Db.Read\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Time\n\n\n");

    result.push_str(
        "field : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b\n",
    );
    result.push_str("field fieldName_ fieldDecoder_ decoder_ =\n");
    result.push_str("    decoder_ |> Decode.andThen (\\func -> Decode.field fieldName_ fieldDecoder_ |> Decode.map func)");

    result.push_str("\n\n");

    for schema in &database.schemas {
        for file in &schema.files {
            for definition in &file.definitions {
                result.push_str(&to_decoder_definition(definition));
            }
        }
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
        ast::Definition::Session(_) => "".to_string(),
        ast::Definition::Comment { .. } => "".to_string(),
        ast::Definition::Tagged { name, variants, .. } => {
            let mut result = "".to_string();

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
            result
        }
        ast::Definition::Record { .. } => "".to_string(),
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

fn to_type_decoder(column: &ast::Column) -> String {
    let decoder = match column.type_.as_str() {
        "String" => "Db.Read.string".to_string(),
        "Int" => "Db.Read.int".to_string(),
        "Float" => "Db.Read.float".to_string(),
        "DateTime" => "Db.Read.dateTime".to_string(),
        "Boolean" => "Db.Read.bool".to_string(),
        _ => format!(
            "Db.Decode.{}",
            crate::ext::string::decapitalize(&column.type_)
        )
        .to_string(),
    };
    if column.nullable {
        format!("(Db.Read.nullable {})", decoder)
    } else {
        decoder
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
pub fn write_queries(
    dir: &Path,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
) -> io::Result<()> {
    let query_dir = dir.join("Query");
    filesystem::create_dir_if_not_exists(&query_dir)?;
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let target_path = query_dir.join(&format!("{}.elm", q.name.to_string()));

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
    result.push_str("import Db.Read\n");
    result.push_str("import Db.Decode\n");
    result.push_str("import Db.Encode\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Json.Encode as Encode\n");
    result.push_str("import Time\n");
    result.push_str("\n\n");

    result.push_str(&to_param_type_alias(&query.args));

    result.push_str(&format!(
        "prepare : Input -> {{ args : Encode.Value, query : String, decoder : Decode.Decoder {} }}\n",
        string::capitalize(&query.name)
    ));
    result.push_str("prepare input =\n");
    result.push_str(&format!(
        "    {{ args = encode input\n    , query = \"{}\"\n    , decoder = decode{}\n    }}\n\n\n",
        &query.interface_hash,
        string::capitalize(&query.name)
    ));

    // Top level query alias
    result.push_str(&format!(
        "{{-| The Return Data! -}}\ntype alias {} =\n",
        crate::ext::string::capitalize(&query.name)
    ));

    let mut is_first = true;
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                if is_first {
                    result.push_str(&format!("    {{ ",))
                }

                let field_name = ast::get_aliased_name(query_field);
                if is_first {
                    result.push_str(&format!(
                        "{} : List {}\n",
                        crate::ext::string::decapitalize(&field_name),
                        string::capitalize(&query_field.name)
                    ));
                } else {
                    let spaces = " ".repeat(4);
                    result.push_str(&format!(
                        "{}, {} : List {}\n",
                        spaces,
                        crate::ext::string::decapitalize(&field_name),
                        string::capitalize(&query_field.name)
                    ));
                }

                if is_first {
                    is_first = false;
                }
            }
            _ => {}
        }
    }
    result.push_str("    }\n\n\n");

    // Helpers

    // Type Alisaes
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let table = context.tables.get(&query_field.name).unwrap();
                result.push_str(&to_query_type_alias(
                    context,
                    &table.record,
                    &query_field.name,
                    &ast::collect_query_fields(&query_field.fields),
                ));
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

    result.push_str(&to_param_type_encoder(&query.args));

    // Top level query decoder
    result.push_str(&to_query_toplevel_decoder(context, &query));
    result.push_str("\n\n");

    // Helper Decoders
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let table = context.tables.get(&query_field.name).unwrap();
                result.push_str(&to_query_decoder(
                    context,
                    &ast::get_aliased_name(&query_field),
                    &table.record,
                    &ast::collect_query_fields(&query_field.fields),
                ));
                result.push_str("\n\n");
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

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

fn to_query_toplevel_decoder(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = format!(
        "decode{} : Decode.Decoder {}\n",
        crate::ext::string::capitalize(&query.name),
        crate::ext::string::capitalize(&query.name)
    );

    result.push_str(&format!(
        "decode{} =\n",
        crate::ext::string::capitalize(&query.name)
    ));
    result.push_str(&format!(
        "    Decode.succeed {}\n",
        crate::ext::string::capitalize(&query.name)
    ));
    for (index, field) in query.fields.iter().enumerate() {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let aliased_field_name = ast::get_aliased_name(query_field);
                result.push_str(&format!(
                    "        |> Db.Read.andDecodeIndex {} decode{}\n",
                    index,
                    crate::ext::string::capitalize(&aliased_field_name)
                ));
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

    result
}

fn to_query_decoder(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> String {
    let mut result = format!(
        "decode{} : Db.Read.Query {}\n",
        crate::ext::string::capitalize(table_alias),
        crate::ext::string::capitalize(table_alias)
    );
    let mut primary_key = vec![];
    match ast::get_primary_id_field_name(&table.fields) {
        Some(id) => primary_key.push(id),
        None => (),
    }

    let identifiers = format!("[ {} ]", format_db_id(table_alias, &primary_key),);

    result.push_str(&format!(
        "decode{} =\n",
        crate::ext::string::capitalize(table_alias)
    ));
    result.push_str(&format!(
        "    Db.Read.query {} {}\n",
        crate::ext::string::capitalize(table_alias),
        identifiers
    ));
    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.push_str(&to_table_field_decoder(
            8,
            table_alias,
            &table_field,
            &field,
        ));
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
                let link_table = typecheck::get_linked_table(context, &link).unwrap();
                result.push_str("\n\n");
                result.push_str(&to_query_decoder(
                    context,
                    &ast::get_aliased_name(&field),
                    &link_table.record,
                    &ast::collect_query_fields(&field.fields),
                ));
            }
            _ => continue,
        }
    }

    result
}

fn format_db_id(table_alias: &str, ids: &Vec<String>) -> String {
    let mut result = String::new();
    for id in ids {
        let formatted = format!("{}__{}", table_alias, id);
        result.push_str(&format!("Db.Read.id \"{}\"", formatted));
    }
    result
}

fn to_table_field_decoder(
    indent: usize,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> String {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}|> Db.Read.field \"{}\" {}\n",
                spaces,
                ast::get_select_alias(table_alias, table_field, query_field),
                to_type_decoder(&column)
            );
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = ast::get_aliased_name(query_field);

            return format!(
                "{}|> Db.Read.nested\n{}({})\n{}({})\n{}decode{}\n",
                spaces,
                // ID columns
                " ".repeat(indent + 4),
                format_db_id(table_alias, &link.local_ids),
                " ".repeat(indent + 4),
                format_db_id(&foreign_table_alias, &link.foreign.fields),
                " ".repeat(indent + 4),
                (crate::ext::string::capitalize(&ast::get_aliased_name(query_field))) // (capitalize(&link.link_name)) // ast::get_select_alias(table_alias, table_field, query_field),
            );
        }

        _ => "".to_string(),
    }
}

fn to_query_type_alias(
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    name: &str,
    fields: &Vec<&ast::QueryField>,
) -> String {
    let mut result = format!("type alias {} =\n", crate::ext::string::capitalize(name));

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
    result.push_str("    }\n\n\n");

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
                let link_table = typecheck::get_linked_table(context, &link).unwrap();

                result.push_str(&to_query_type_alias(
                    context,
                    &link_table.record,
                    &ast::get_aliased_name(field),
                    &ast::collect_query_fields(&field.fields),
                ));
                // result.push_str("\n\n");
            }
            _ => continue,
        }
    }

    result
}

fn to_string_query_field_link(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    link_details: &ast::LinkDetails,
) -> String {
    let field_name = ast::get_aliased_name(field);

    if is_first {
        return format!(
            "{} : {}\n",
            crate::ext::string::decapitalize(&field_name),
            (format!("List {}", crate::ext::string::capitalize(&field_name)))
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}\n",
            spaces,
            crate::ext::string::decapitalize(&field_name),
            (format!("List {}", crate::ext::string::capitalize(&field_name)))
        );
    }
}

fn to_string_query_field(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    table_column: &ast::Column,
) -> String {
    let field_name = ast::get_aliased_name(field);
    let maybe = if table_column.nullable { "Maybe " } else { "" };
    if is_first {
        return format!(
            "{} : {}{}\n",
            crate::ext::string::decapitalize(&field_name),
            maybe,
            to_elm_typename(&table_column.type_)
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}{}\n",
            spaces,
            crate::ext::string::decapitalize(&field_name),
            maybe,
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
        "DateTime" => "Time.Posix".to_string(),
        _ => format!("Db.{}", type_).to_string(),
    }
}
