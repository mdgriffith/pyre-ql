use crate::ast;
use crate::ext::string;
use crate::filesystem::{generate_text_file, GeneratedFile};

use crate::generate::typealias;
use crate::typecheck;
use std::path::Path;

mod rectangle;

const ELM_DELTA_MODULE: &str = include_str!("./static/elm/src/Db/Delta.elm");

pub fn generate(
    base_path: &Path,
    database: &ast::Database,
    files: &mut Vec<GeneratedFile<String>>,
) {
    files.push(generate_text_file(
        base_path.join("Db/Decode.elm"),
        to_schema_decoders(database),
    ));
    files.push(generate_text_file(
        base_path.join("Db/Encode.elm"),
        to_schema_encoders(database),
    ));
    files.push(generate_text_file(
        base_path.join("Db/Delta.elm"),
        ELM_DELTA_MODULE,
    ));
}

pub fn write_schema(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("module Db exposing (..)\n\nimport Time\n\n\n");

    // Generate generic branded ID types
    result.push_str("-- Generic branded ID types\n");
    result.push_str("type Id brand = Id Int\n");
    result.push_str("type Uuid brand = Uuid String\n\n\n");

    // Collect all unique brands from ID columns
    let brands = collect_brands(database);

    // Generate phantom types for each brand
    if !brands.is_empty() {
        result.push_str("-- Phantom types for each table/entity\n");
        for brand in &brands {
            result.push_str(&format!("type {} = {}\n", brand, brand));
        }
        result.push_str("\n\n");

        // Generate type aliases for each brand
        result.push_str("-- Table-specific ID aliases\n");
        for brand in &brands {
            result.push_str(&format!("type alias {}Id = Id {}\n", brand, brand));
        }
        result.push_str("\n\n");
    }

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

/// Collect all unique brands from ID columns in the database
fn collect_brands(database: &ast::Database) -> Vec<String> {
    use std::collections::HashSet;
    let mut brands = HashSet::new();

    for schema in &database.schemas {
        for file in &schema.files {
            for definition in &file.definitions {
                if let ast::Definition::Record { fields, .. } = definition {
                    for field in fields {
                        if let ast::Field::Column(column) = field {
                            // Check if this is an ID type with a brand (non-empty table name)
                            if let Some(brand) = column.type_.table_name() {
                                brands.insert(brand.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut brands_vec: Vec<String> = brands.into_iter().collect();
    brands_vec.sort();
    brands_vec
}

fn to_string_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => {
            if *count > 2 {
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
        if ast::is_column_space(field) {
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
            if *count > 2 {
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
    let type_str = column_to_elm_type(column);

    if is_first {
        return format!("{} : {}{}\n", column.name, maybe, type_str);
    } else {
        let spaces = " ".repeat(indent);
        return format!("{}, {} : {}{}\n", spaces, column.name, maybe, type_str);
    }
}

/// Convert a column to its Elm type representation
/// For ID types with brands, generates branded types like `UserId` or `Uuid User`
fn column_to_elm_type(column: &ast::Column) -> String {
    match &column.type_ {
        ast::ColumnType::IdInt { table } => {
            if !table.is_empty() {
                format!("{}Id", table)
            } else {
                "Int".to_string()
            }
        }
        ast::ColumnType::IdUuid { table } => {
            if !table.is_empty() {
                format!("Uuid {}", table)
            } else {
                "String".to_string()
            }
        }
        _ => column.type_.to_string(),
    }
}

// DECODE

pub fn to_schema_decoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("module Db.Decode exposing (..)\n\n");

    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Time\n\n\n");

    result.push_str(
        r#"field : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
field fieldName_ fieldDecoder_ decoder_ =
    decoder_ |> Decode.andThen (\func -> Decode.field fieldName_ fieldDecoder_ |> Decode.map func)


{-| Chain field decoders together, similar to Db.Read.field.
This allows you to build up a decoder by adding fields one at a time.

    decodeGame =
        Decode.succeed Game
            |> andField "id" Decode.int 
            |> andField "name" Decode.string

-}
andField : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andField fieldName decoder partial =
    Decode.map2 (\f value -> f value)
        partial
        (Decode.field fieldName decoder)


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
        ast::Definition::Lines { .. } => (),
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

            for variant in variants {
                result.push_str(&to_decoder_variant(18, name, variant));
            }
            result.push_str("            )\n");
            result.push_str("        |> Db.Read.custom\n");
        }
        ast::Definition::Record { .. } => (),
    }
}

fn to_decoder_variant(indent_size: usize, typename: &str, variant: &ast::Variant) -> String {
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

fn to_json_type_decoder(type_: &ast::ColumnType) -> String {
    match type_ {
        ast::ColumnType::String => "Decode.string".to_string(),
        ast::ColumnType::Int => "Decode.int".to_string(),
        ast::ColumnType::Float => "Decode.float".to_string(),
        ast::ColumnType::DateTime => "Db.Read.dateTime".to_string(),
        _ => crate::ext::string::decapitalize(&type_.to_string()).to_string(),
    }
}

// Encoders!
//
pub fn to_schema_encoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str(
        "module Db.Encode exposing (..)\n\nimport Json.Encode as Encode\nimport Time\n\n\n",
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
        ast::Definition::Lines { .. } => "".to_string(),
        ast::Definition::Comment { .. } => "".to_string(),
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

            for variant in variants {
                result.push_str(&to_encoder_variant(8, name, variant));
            }
            result
        }
        ast::Definition::Record { .. } => "".to_string(),
    }
}

fn to_encoder_variant(indent_size: usize, _typename: &str, variant: &ast::Variant) -> String {
    let outer_indent = " ".repeat(indent_size);
    let indent = " ".repeat(indent_size + 4);
    let inner_indent = " ".repeat(indent_size + 8);
    match &variant.fields {
        Some(fields) => {
            let mut result = format!(
                "{}Db.{} inner_details__ ->\n{}Encode.object\n{}[ ( \"type\", Encode.string \"{}\" )\n",
                outer_indent, variant.name, indent, inner_indent, variant.name
            );

            for field in fields {
                result.push_str(&to_field_encoder(indent_size + 8, &field));
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

fn to_field_encoder(indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}, ( \"{}\", {} inner_details__.{})\n",
                spaces,
                column.name,
                to_type_encoder(&column.type_),
                column.name
            );
        }

        _ => "".to_string(),
    }
}

fn to_type_encoder(type_: &ast::ColumnType) -> String {
    match type_ {
        ast::ColumnType::String => "Encode.string".to_string(),
        ast::ColumnType::Int => "Encode.int".to_string(),
        ast::ColumnType::Float => "Encode.float".to_string(),
        ast::ColumnType::DateTime => "Db.Encode.dateTime".to_string(),
        _ => format!("Db.Encode.{}", string::decapitalize(&type_.to_string())).to_string(),
    }
}

fn to_type_encoder_str(type_: &str) -> String {
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
    let mut query_names: Vec<String> = Vec::new();

    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                // Only generate QueryClient for select queries, not mutations
                if q.operation == ast::QueryOperation::Query {
                    query_names.push(q.name.clone());
                }
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

    // Generate the Pyre.elm module that ties all queries together
    if !query_names.is_empty() {
        files.push(generate_text_file(
            base_out_dir.join("Pyre.elm"),
            generate_pyre_module(context, query_list, &query_names),
        ));
    }
}

fn to_query_file(context: &typecheck::Context, query: &ast::Query) -> String {
    // Collect type names as we generate them
    use std::cell::RefCell;
    use std::rc::Rc;

    let type_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Always include Input
    type_names.borrow_mut().push("Input".to_string());

    let mut result = String::new();

    result.push_str("import Db.Decode\n");
    result.push_str("import Db.Delta\n");
    result.push_str("import Db.Encode\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Json.Encode as Encode\n");
    result.push_str("import Time\n");
    result.push_str("\n\n");

    result.push_str(&to_param_type_alias(&query.args));

    let type_names_clone = type_names.clone();
    let formatter = typealias::TypeFormatter {
        to_comment: Box::new(|s| format!("{{-| {} -}}\n", s)),
        to_type_def_start: Box::new(move |name| {
            type_names_clone.borrow_mut().push(name.to_string());
            format!("type alias {} =\n    {{ ", name)
        }),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
                 is_array_relationship: _,
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
                "decode{} : Decode.Decoder {}\ndecode{} =\n    Decode.succeed {}\n        ",
                name, name, name, name
            )
        }),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
                 is_array_relationship: _,
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

                format!("|> Db.Decode.andField \"{}\" {}\n", name, final_decoder)
            },
        ),
        to_type_def_end: Box::new(|| "\n".to_string()),
        to_field_separator: Box::new(|_| "        ".to_string()),
    };

    result.push_str(&to_param_type_encoder(&query.args));

    // Top level query decoder

    typealias::return_data_aliases(context, query, &mut result, &decoder_formatter);

    // Generate queryShape as JSON encoder (only for queries)
    if query.operation == ast::QueryOperation::Query {
        result.push_str("\n\n");
        result.push_str(&to_query_shape_json(context, query));
    }

    // Generate QueryDelta types and application code (only for queries, not mutations)
    if query.operation == ast::QueryOperation::Query {
        result.push_str(&to_query_delta_types(context, query));
    }

    // Build exposing list: functions first, then types
    let mut exposing_items: Vec<String> = Vec::new();

    // Add encoder function
    exposing_items.push("encode".to_string());

    // Add delta functions and types for queries (not mutations)
    if query.operation == ast::QueryOperation::Query {
        exposing_items.push("applyDelta".to_string());
        exposing_items.push("decodeQueryDelta".to_string());
        exposing_items.push("QueryDelta(..)".to_string());
    }

    // Then add types (sorted)
    let mut type_names_sorted: Vec<String> = type_names.borrow().clone();
    type_names_sorted.sort();
    exposing_items.extend(type_names_sorted);

    // Build the module declaration with explicit exposing
    let exposing_list = exposing_items.join(", ");
    let module_decl = format!(
        "module Query.{} exposing ({})\n\n\n",
        query.name, exposing_list
    );

    // Replace the placeholder or prepend the module declaration
    // Since we started with an empty result, we need to prepend
    format!("{}{}", module_decl, result)
}

/// Generate an empty ReturnData constructor with the right number of empty lists
fn generate_empty_return_data(query: &ast::Query) -> String {
    let field_count = query
        .fields
        .iter()
        .filter(|f| matches!(f, ast::TopLevelQueryField::Field(_)))
        .count();

    if field_count == 0 {
        "ReturnData".to_string()
    } else {
        let empty_lists = vec!["[]"; field_count].join(" ");
        format!("ReturnData {}", empty_lists)
    }
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
                to_type_encoder_str(&type_string),
                &arg.name
            ));
            is_first = false;
        } else {
            result.push_str(&format!(
                "        , ( {}, {} input.{})\n",
                string::quote(&arg.name),
                to_type_encoder_str(&type_string),
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

fn to_query_shape_json(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = "queryShape : Encode.Value\n".to_string();
    result.push_str("queryShape =\n");
    result.push_str("    Encode.object\n");
    result.push_str("        [ ");

    let mut is_first_table = true;
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                if !is_first_table {
                    result.push_str("\n        , ");
                }
                is_first_table = false;

                let field_name = ast::get_aliased_name(query_field);
                result.push_str(&format!(
                    "({}, {})",
                    string::quote(&field_name),
                    to_query_field_spec_json(context, query_field)
                ));
            }
            _ => {}
        }
    }

    result.push_str("\n        ]\n");
    result
}

fn to_query_field_spec_json(context: &typecheck::Context, query_field: &ast::QueryField) -> String {
    let mut result = "Encode.object\n            [ ".to_string();
    let mut is_first = true;

    // Get table info for relationship detection
    let table = context.tables.get(&query_field.name);

    // Extract special directives (@where, @sort, @limit)
    let mut sort_clauses: Vec<String> = Vec::new();
    let mut limit: Option<i32> = None;

    // Collect all field selections and args
    let mut field_selections: Vec<(String, bool, bool)> = Vec::new();

    for arg_field in &query_field.fields {
        match arg_field {
            ast::ArgField::Arg(located_arg) => {
                match &located_arg.arg {
                    ast::Arg::Where(_where_arg) => {
                        // TODO: Convert WhereArg to QueryShape where clause format
                        // For now, skip - this is complex and may need runtime evaluation
                    }
                    ast::Arg::OrderBy(direction, field_name) => {
                        let dir_str = match direction {
                            ast::Direction::Asc => "asc",
                            ast::Direction::Desc => "desc",
                        };
                        sort_clauses.push(format!(
                            "Encode.object [ (\"field\", Encode.string {}) , (\"direction\", Encode.string {}) ]",
                            string::quote(field_name),
                            string::quote(dir_str)
                        ));
                    }
                    ast::Arg::Limit(query_value) => {
                        if let ast::QueryValue::Int((_, val)) = query_value {
                            limit = Some(*val);
                        }
                    }
                }
            }
            ast::ArgField::Field(nested_field) => {
                let is_relationship = if let Some(table_info) = table {
                    let links = ast::collect_links(&table_info.record.fields);
                    links.iter().any(|link| link.link_name == nested_field.name)
                } else {
                    false
                };

                let has_nested_fields = !nested_field.fields.is_empty()
                    && nested_field
                        .fields
                        .iter()
                        .any(|f| matches!(f, ast::ArgField::Field(_)));

                field_selections.push((
                    nested_field.name.clone(),
                    is_relationship,
                    has_nested_fields,
                ));
            }
            _ => {}
        }
    }

    // Generate field selections
    for (field_name, is_relationship, has_nested_fields) in field_selections {
        if !is_first {
            result.push_str("\n            , ");
        }
        is_first = false;

        if is_relationship && has_nested_fields {
            // Relationship field with nested selections - recurse
            if let Some(nested_field) = query_field.fields.iter().find_map(|f| match f {
                ast::ArgField::Field(qf) if qf.name == field_name => Some(qf),
                _ => None,
            }) {
                result.push_str(&format!(
                    "({}, {})",
                    string::quote(&field_name),
                    to_query_field_spec_json(context, nested_field)
                ));
            }
        } else {
            // Regular field or relationship without nested - just true
            result.push_str(&format!(
                "({}, Encode.bool True)",
                string::quote(&field_name)
            ));
        }
    }

    // Add special directives if present
    if !sort_clauses.is_empty() || limit.is_some() {
        if !is_first {
            result.push_str("\n            , ");
        }

        if !sort_clauses.is_empty() {
            if sort_clauses.len() == 1 {
                result.push_str(&format!("(\"@sort\", {})", sort_clauses[0]));
            } else {
                // For multiple sort clauses, create a list
                result.push_str("(\"@sort\", Encode.list identity [");
                for (i, clause) in sort_clauses.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(clause);
                }
                result.push_str("])");
            }
        }

        if let Some(limit_val) = limit {
            if !sort_clauses.is_empty() {
                result.push_str("\n            , ");
            }
            result.push_str(&format!("(\"@limit\", Encode.int {})", limit_val));
        }
    }

    result.push_str("\n            ]");
    result
}

// QUERY DELTA TYPES AND APPLICATION CODE
//
// Generates types and functions for applying deltas to query results using lenses

fn to_query_delta_types(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = String::new();

    result.push_str("\n\n-- QueryDelta Types\n\n\n");

    // QueryDelta envelope type (uses DeltaOp from Db.Delta)
    result.push_str("type QueryDelta\n");
    result.push_str("    = Full Int ReturnData\n");
    result.push_str("    | Delta Int (List Db.Delta.DeltaOp)\n\n\n");

    // QueryDelta decoder (uses Db.Delta.decodeDeltaOp)
    result.push_str("decodeQueryDelta : Decode.Decoder QueryDelta\n");
    result.push_str("decodeQueryDelta =\n");
    result.push_str("    Decode.field \"type\" Decode.string\n");
    result.push_str("        |> Decode.andThen\n");
    result.push_str("            (\\type_ ->\n");
    result.push_str("                case type_ of\n");
    result.push_str("                    \"full\" ->\n");
    result.push_str("                        Decode.map2 Full\n");
    result.push_str("                            (Decode.field \"revision\" Decode.int)\n");
    result.push_str("                            (Decode.field \"result\" decodeReturnData)\n\n");
    result.push_str("                    \"delta\" ->\n");
    result.push_str("                        Decode.map2 Delta\n");
    result.push_str("                            (Decode.field \"revision\" Decode.int)\n");
    result.push_str("                            (Decode.at [ \"delta\", \"ops\" ] (Decode.list Db.Delta.decodeDeltaOp))\n\n");
    result.push_str("                    _ ->\n");
    result
        .push_str("                        Decode.fail (\"Unknown QueryDelta type: \" ++ type_)\n");
    result.push_str("            )\n\n\n");

    // Generate applyDelta function with lens-based approach
    result.push_str(&to_apply_delta_function_with_lenses(context, query));

    result
}

/// Generate the applyDelta function using the lens-based approach
fn to_apply_delta_function_with_lenses(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = String::new();

    // applyDelta function
    result.push_str("applyDelta : QueryDelta -> ReturnData -> Result String ReturnData\n");
    result.push_str("applyDelta delta data =\n");
    result.push_str("    case delta of\n");
    result.push_str("        Full _ newData ->\n");
    result.push_str("            Ok newData\n\n");
    result.push_str("        Delta _ ops ->\n");
    result.push_str("            Db.Delta.applyOps fields ops data\n\n\n");

    // Generate field lenses section header
    result.push_str("-- Field Lenses (generated)\n\n\n");

    // Generate the fields list for ReturnData
    result.push_str("fields : List ( String, Db.Delta.FieldHandler ReturnData )\n");
    result.push_str("fields =\n");

    let mut field_entries: Vec<String> = Vec::new();
    for field in &query.fields {
        if let ast::TopLevelQueryField::Field(query_field) = field {
            let field_name = ast::get_aliased_name(query_field);
            // Top-level query fields are always List, so use listField
            field_entries.push(format!(
                "( \"{}\", Db.Delta.listField {}Lens )",
                field_name, field_name
            ));
        }
    }

    if field_entries.is_empty() {
        result.push_str("    []\n\n\n");
    } else {
        result.push_str("    [ ");
        result.push_str(&field_entries.join("\n    , "));
        result.push_str("\n    ]\n\n\n");
    }

    // Generate lens definitions for each top-level field
    for field in &query.fields {
        if let ast::TopLevelQueryField::Field(query_field) = field {
            result.push_str(&generate_field_lens(context, query_field, "ReturnData", ""));
        }
    }

    result
}

/// Generate a lens definition for a field and its nested fields recursively
fn generate_field_lens(
    context: &typecheck::Context,
    query_field: &ast::QueryField,
    parent_type: &str,
    alias_stack: &str,
) -> String {
    let mut result = String::new();
    let field_name = ast::get_aliased_name(query_field);
    let capitalized = string::capitalize(&field_name);

    // Build the type name for this field
    let type_name = if alias_stack.is_empty() {
        capitalized.clone()
    } else {
        format!("{}_{}", alias_stack, capitalized)
    };

    // Top-level fields are always lists
    result.push_str(&format!(
        "{}Lens : Db.Delta.ListLens {} {}\n",
        field_name, parent_type, type_name
    ));
    result.push_str(&format!("{}Lens =\n", field_name));
    result.push_str(&format!("    {{ get = .{}\n", field_name));
    result.push_str(&format!(
        "    , set = \\list data -> {{ data | {} = list }}\n",
        field_name
    ));
    result.push_str(&format!("    , decode = decode{}\n", type_name));

    // Generate nested field lookup function
    let nested_fields = collect_nested_fields_with_type(context, query_field);

    if nested_fields.is_empty() {
        result.push_str("    , nested = Db.Delta.noNested\n");
    } else {
        result.push_str(&format!("    , nested = {}NestedFields\n", field_name));
    }
    result.push_str("    }\n\n\n");

    // Generate nested fields lookup function if there are nested fields
    if !nested_fields.is_empty() {
        result.push_str(&format!(
            "{}NestedFields : String -> Maybe (Db.Delta.FieldHandler {})\n",
            field_name, type_name
        ));
        result.push_str(&format!("{}NestedFields name =\n", field_name));
        result.push_str("    case name of\n");

        for (nested_name, nested_type, is_optional) in &nested_fields {
            let lens_constructor = if *is_optional {
                "Db.Delta.maybeField"
            } else {
                "Db.Delta.listField"
            };
            result.push_str(&format!(
                "        \"{}\" ->\n            Just ({} {}Lens)\n\n",
                nested_name, lens_constructor, nested_name
            ));
        }
        result.push_str("        _ ->\n            Nothing\n\n\n");

        // Recursively generate lenses for nested fields
        for (nested_name, nested_type, is_optional) in &nested_fields {
            if let Some(nested_query_field) = find_nested_query_field(query_field, nested_name) {
                result.push_str(&generate_nested_field_lens(
                    context,
                    nested_query_field,
                    &type_name,
                    nested_type,
                    *is_optional,
                ));
            }
        }
    }

    result
}

/// Generate a lens for a nested field
fn generate_nested_field_lens(
    context: &typecheck::Context,
    query_field: &ast::QueryField,
    parent_type: &str,
    type_name: &str,
    is_optional: bool,
) -> String {
    let mut result = String::new();
    let field_name = ast::get_aliased_name(query_field);

    if is_optional {
        // Maybe lens
        result.push_str(&format!(
            "{}Lens : Db.Delta.MaybeLens {} {}\n",
            field_name, parent_type, type_name
        ));
        result.push_str(&format!("{}Lens =\n", field_name));
        result.push_str(&format!("    {{ get = .{}\n", field_name));
        result.push_str(&format!(
            "    , set = \\val item -> {{ item | {} = val }}\n",
            field_name
        ));
    } else {
        // List lens
        result.push_str(&format!(
            "{}Lens : Db.Delta.ListLens {} {}\n",
            field_name, parent_type, type_name
        ));
        result.push_str(&format!("{}Lens =\n", field_name));
        result.push_str(&format!("    {{ get = .{}\n", field_name));
        result.push_str(&format!(
            "    , set = \\list item -> {{ item | {} = list }}\n",
            field_name
        ));
    }
    result.push_str(&format!("    , decode = decode{}\n", type_name));

    // Check for further nested fields
    let nested_fields = collect_nested_fields_with_type(context, query_field);

    if nested_fields.is_empty() {
        result.push_str("    , nested = Db.Delta.noNested\n");
    } else {
        result.push_str(&format!("    , nested = {}NestedFields\n", field_name));
    }
    result.push_str("    }\n\n\n");

    // Generate nested fields lookup if needed
    if !nested_fields.is_empty() {
        result.push_str(&format!(
            "{}NestedFields : String -> Maybe (Db.Delta.FieldHandler {})\n",
            field_name, type_name
        ));
        result.push_str(&format!("{}NestedFields name =\n", field_name));
        result.push_str("    case name of\n");

        for (nested_name, nested_nested_type, nested_is_optional) in &nested_fields {
            let lens_constructor = if *nested_is_optional {
                "Db.Delta.maybeField"
            } else {
                "Db.Delta.listField"
            };
            result.push_str(&format!(
                "        \"{}\" ->\n            Just ({} {}Lens)\n\n",
                nested_name, lens_constructor, nested_name
            ));
        }
        result.push_str("        _ ->\n            Nothing\n\n\n");

        // Recursively generate lenses for deeply nested fields
        for (nested_name, nested_nested_type, nested_is_optional) in &nested_fields {
            if let Some(nested_query_field) = find_nested_query_field(query_field, nested_name) {
                result.push_str(&generate_nested_field_lens(
                    context,
                    nested_query_field,
                    type_name,
                    nested_nested_type,
                    *nested_is_optional,
                ));
            }
        }
    }

    result
}

/// Collect nested fields with their type information (name, type_name, is_optional)
fn collect_nested_fields_with_type(
    context: &typecheck::Context,
    query_field: &ast::QueryField,
) -> Vec<(String, String, bool)> {
    let mut nested = Vec::new();
    let parent_name = ast::get_aliased_name(query_field);
    let parent_capitalized = string::capitalize(&parent_name);

    // Get the table for this query field to determine relationship types
    // context.tables is a HashMap<String, Table>, so iter() yields (&String, &Table)
    let table = context
        .tables
        .iter()
        .find(|(name, _)| *name == &query_field.name)
        .map(|(_, t)| t);

    for arg_field in &query_field.fields {
        if let ast::ArgField::Field(nested_field) = arg_field {
            // Check if this nested field has its own fields (making it a relationship)
            let has_fields = nested_field
                .fields
                .iter()
                .any(|f| matches!(f, ast::ArgField::Field(_)));
            if has_fields {
                let nested_name = ast::get_aliased_name(nested_field);
                let nested_type = format!(
                    "{}_{}",
                    parent_capitalized,
                    string::capitalize(&nested_name)
                );

                // Determine if this relationship is optional (Maybe) or an array (List)
                let is_optional = if let Some(table) = table {
                    is_relationship_optional(context, table, &nested_field.name)
                } else {
                    false // Default to List if we can't determine
                };

                nested.push((nested_name, nested_type, is_optional));
            }
        }
    }
    nested
}

/// Determine if a relationship field should be Maybe (optional) or List
fn is_relationship_optional(
    context: &typecheck::Context,
    table: &typecheck::Table,
    field_name: &str,
) -> bool {
    // Find the link directive for this field
    for f in &table.record.fields {
        if let ast::Field::FieldDirective(ast::FieldDirective::Link(link)) = f {
            if link.link_name == field_name {
                // Determine relationship type
                let primary_key_name = ast::get_primary_id_field_name(&table.record.fields);
                let is_one_to_many = link.local_ids.iter().all(|id| {
                    primary_key_name
                        .as_ref()
                        .map(|pk| id == pk)
                        .unwrap_or(false)
                });

                if is_one_to_many {
                    return false; // One-to-many is List, not optional
                }

                // Check if link points to unique fields
                let linked_to_unique =
                    if let Some(linked_table) = typecheck::get_linked_table(context, link) {
                        ast::linked_to_unique_field_with_record(link, &linked_table.record)
                    } else {
                        ast::linked_to_unique_field(link)
                    };

                return linked_to_unique; // Many-to-one or one-to-one pointing to unique is optional
            }
        }
    }
    false // Default to List
}

/// Find a nested query field by name
fn find_nested_query_field<'a>(
    query_field: &'a ast::QueryField,
    name: &str,
) -> Option<&'a ast::QueryField> {
    for arg_field in &query_field.fields {
        if let ast::ArgField::Field(nested_field) = arg_field {
            if nested_field.name == name {
                return Some(nested_field);
            }
        }
    }
    None
}

// PYRE.ELM MODULE GENERATION
//
// Generates the main QueryClient module that ties all queries together

fn generate_pyre_module(
    _context: &typecheck::Context,
    query_list: &ast::QueryList,
    query_names: &[String],
) -> String {
    let mut result = String::new();

    // Module declaration
    result.push_str(
        "port module Pyre exposing (Model, Msg(..), init, update, subscriptions, getResult)\n\n\n",
    );

    // Imports
    result.push_str("import Dict exposing (Dict)\n");
    result.push_str("import Json.Decode as Decode\n");
    result.push_str("import Json.Encode as Encode\n");

    // Import query modules
    for name in query_names {
        result.push_str(&format!("import Query.{}\n", name));
    }
    result.push_str("\n\n");

    // Model type
    result.push_str("-- Model\n\n\n");
    result.push_str("type alias Model =\n");
    result.push_str("    {");

    for (i, name) in query_names.iter().enumerate() {
        let field_name = string::decapitalize(name);
        if i == 0 {
            result.push_str(&format!(
                " {} : Dict String (QueryModel Query.{}.Input Query.{}.ReturnData)\n",
                field_name, name, name
            ));
        } else {
            result.push_str(&format!(
                "    , {} : Dict String (QueryModel Query.{}.Input Query.{}.ReturnData)\n",
                field_name, name, name
            ));
        }
    }

    result.push_str("    }\n\n\n");

    // QueryModel type
    result.push_str("type alias QueryModel input result =\n");
    result.push_str("    { input : input\n");
    result.push_str("    , result : result\n");
    result.push_str("    , revision : Int\n");
    result.push_str("    }\n\n\n");

    // Msg type
    result.push_str("-- Msg\n\n\n");
    result.push_str("type Msg\n");
    for (i, name) in query_names.iter().enumerate() {
        let prefix = if i == 0 { "    = " } else { "    | " };
        result.push_str(&format!(
            "{}{}_Registered String Query.{}.Input\n",
            prefix, name, name
        ));
        result.push_str(&format!(
            "    | {}_InputUpdated String Query.{}.Input\n",
            name, name
        ));
        result.push_str(&format!(
            "    | {}_DataReceived String Query.{}.QueryDelta\n",
            name, name
        ));
        result.push_str(&format!("    | {}_Unregistered String\n", name));
    }
    result.push_str("\n\n");

    // Ports
    result.push_str("-- Ports\n\n\n");
    result.push_str("port pyre_sendQueryClientMessage : Encode.Value -> Cmd msg\n\n\n");
    result.push_str("port pyre_receiveQueryDelta : (Decode.Value -> msg) -> Sub msg\n\n\n");
    result.push_str("port pyre_logQueryDeltaError : Encode.Value -> Cmd msg\n\n\n");

    // Init
    result.push_str("-- Init\n\n\n");
    result.push_str("init : Model\n");
    result.push_str("init =\n");
    result.push_str("    {");

    for (i, name) in query_names.iter().enumerate() {
        let field_name = string::decapitalize(name);
        if i == 0 {
            result.push_str(&format!(" {} = Dict.empty\n", field_name));
        } else {
            result.push_str(&format!("    , {} = Dict.empty\n", field_name));
        }
    }

    result.push_str("    }\n\n\n");

    // Update
    result.push_str("-- Update\n\n\n");
    result.push_str("update : Msg -> Model -> ( Model, Cmd Msg )\n");
    result.push_str("update msg model =\n");
    result.push_str("    case msg of\n");

    for name in query_names {
        let field_name = string::decapitalize(name);

        // Look up the query to get field count for empty ReturnData
        let empty_return_data = query_list
            .queries
            .iter()
            .find_map(|q| match q {
                ast::QueryDef::Query(query) if query.name == *name => {
                    Some(generate_empty_return_data(query))
                }
                _ => None,
            })
            .unwrap_or_else(|| "ReturnData".to_string());

        // Registered
        result.push_str(&format!("        {}_Registered queryId input ->\n", name));
        result.push_str("            let\n");
        result.push_str(&format!("                queryModel =\n"));
        result.push_str(&format!(
            "                    {{ input = input, result = Query.{}.{}, revision = 0 }}\n",
            name, empty_return_data
        ));
        result.push_str("            in\n");
        result.push_str(&format!(
            "            ( {{ model | {} = Dict.insert queryId queryModel model.{} }}\n",
            field_name, field_name
        ));
        result.push_str(&format!(
            "            , pyre_sendQueryClientMessage (encodeRegister \"{}\" queryId (Query.{}.encode input))\n",
            name, name
        ));
        result.push_str("            )\n\n");

        // InputUpdated
        result.push_str(&format!("        {}_InputUpdated queryId input ->\n", name));
        result.push_str(&format!(
            "            case Dict.get queryId model.{} of\n",
            field_name
        ));
        result.push_str("                Just queryModel ->\n");
        result.push_str(&format!(
            "                    ( {{ model | {} = Dict.insert queryId {{ queryModel | input = input }} model.{} }}\n",
            field_name, field_name
        ));
        result.push_str(&format!(
            "                    , pyre_sendQueryClientMessage (encodeUpdateInput queryId (Query.{}.encode input))\n",
            name
        ));
        result.push_str("                    )\n\n");
        result.push_str("                Nothing ->\n");
        result.push_str("                    ( model, Cmd.none )\n\n");

        // DataReceived
        result.push_str(&format!("        {}_DataReceived queryId delta ->\n", name));
        result.push_str(&format!(
            "            case Dict.get queryId model.{} of\n",
            field_name
        ));
        result.push_str("                Just queryModel ->\n");
        result.push_str(&format!(
            "                    case Query.{}.applyDelta delta queryModel.result of\n",
            name
        ));
        result.push_str("                        Ok newResult ->\n");
        result.push_str("                            let\n");
        result.push_str("                                newRevision =\n");
        result.push_str("                                    case delta of\n");
        result.push_str(&format!(
            "                                        Query.{}.Full rev _ ->\n",
            name
        ));
        result.push_str("                                            rev\n\n");
        result.push_str(&format!(
            "                                        Query.{}.Delta rev _ ->\n",
            name
        ));
        result.push_str("                                            rev\n");
        result.push_str("                            in\n");
        result.push_str(&format!(
            "                            ( {{ model | {} = Dict.insert queryId {{ queryModel | result = newResult, revision = newRevision }} model.{} }}\n",
            field_name, field_name
        ));
        result.push_str("                            , Cmd.none\n");
        result.push_str("                            )\n\n");
        result.push_str("                        Err errMsg ->\n");
        result.push_str("                            ( model\n");
        result.push_str(
            "                            , pyre_logQueryDeltaError (encodeError queryId errMsg)\n",
        );
        result.push_str("                            )\n\n");
        result.push_str("                Nothing ->\n");
        result.push_str("                    ( model, Cmd.none )\n\n");

        // Unregistered
        result.push_str(&format!("        {}_Unregistered queryId ->\n", name));
        result.push_str(&format!(
            "            ( {{ model | {} = Dict.remove queryId model.{} }}\n",
            field_name, field_name
        ));
        result.push_str("            , pyre_sendQueryClientMessage (encodeUnregister queryId)\n");
        result.push_str("            )\n\n");
    }

    result.push_str("\n");

    // Subscriptions
    result.push_str("-- Subscriptions\n\n\n");
    result.push_str("subscriptions : Sub Msg\n");
    result.push_str("subscriptions =\n");
    result.push_str("    pyre_receiveQueryDelta decodeIncomingDelta\n\n\n");

    result.push_str("decodeIncomingDelta : Decode.Value -> Msg\n");
    result.push_str("decodeIncomingDelta json =\n");
    result.push_str("    case Decode.decodeValue incomingDeltaDecoder json of\n");
    result.push_str("        Ok msg ->\n");
    result.push_str("            msg\n\n");
    result.push_str("        Err _ ->\n");
    // Default to first query's unregistered message as fallback
    if let Some(first_name) = query_names.first() {
        result.push_str(&format!(
            "            {}_Unregistered \"\"\n\n\n",
            first_name
        ));
    } else {
        result.push_str("            -- No queries available\n\n\n");
    }

    result.push_str("incomingDeltaDecoder : Decode.Decoder Msg\n");
    result.push_str("incomingDeltaDecoder =\n");
    result.push_str("    Decode.map2 Tuple.pair\n");
    result.push_str("        (Decode.field \"querySource\" Decode.string)\n");
    result.push_str("        (Decode.field \"queryId\" Decode.string)\n");
    result.push_str("        |> Decode.andThen\n");
    result.push_str("            (\\( source, queryId ) ->\n");
    result.push_str("                case source of\n");

    for name in query_names {
        result.push_str(&format!("                    \"{}\" ->\n", name));
        result.push_str(&format!(
            "                        Decode.map ({}_DataReceived queryId) Query.{}.decodeQueryDelta\n\n",
            name, name
        ));
    }

    result.push_str("                    _ ->\n");
    result.push_str("                        Decode.fail (\"Unknown query source: \" ++ source)\n");
    result.push_str("            )\n\n\n");

    // getResult helper
    result.push_str("-- Helpers\n\n\n");
    result
        .push_str("getResult : String -> Dict String (QueryModel input result) -> Maybe result\n");
    result.push_str("getResult queryId queries =\n");
    result.push_str("    Dict.get queryId queries\n");
    result.push_str("        |> Maybe.map .result\n\n\n");

    // Encoders
    result.push_str("-- Encoders\n\n\n");

    result.push_str("encodeRegister : String -> String -> Encode.Value -> Encode.Value\n");
    result.push_str("encodeRegister querySource queryId input =\n");
    result.push_str("    Encode.object\n");
    result.push_str("        [ ( \"type\", Encode.string \"register\" )\n");
    result.push_str("        , ( \"querySource\", Encode.string querySource )\n");
    result.push_str("        , ( \"queryId\", Encode.string queryId )\n");
    result.push_str("        , ( \"queryInput\", input )\n");
    result.push_str("        ]\n\n\n");

    result.push_str("encodeUpdateInput : String -> Encode.Value -> Encode.Value\n");
    result.push_str("encodeUpdateInput queryId input =\n");
    result.push_str("    Encode.object\n");
    result.push_str("        [ ( \"type\", Encode.string \"update-input\" )\n");
    result.push_str("        , ( \"queryId\", Encode.string queryId )\n");
    result.push_str("        , ( \"queryInput\", input )\n");
    result.push_str("        ]\n\n\n");

    result.push_str("encodeUnregister : String -> Encode.Value\n");
    result.push_str("encodeUnregister queryId =\n");
    result.push_str("    Encode.object\n");
    result.push_str("        [ ( \"type\", Encode.string \"unregister\" )\n");
    result.push_str("        , ( \"queryId\", Encode.string queryId )\n");
    result.push_str("        ]\n\n\n");

    result.push_str("encodeError : String -> String -> Encode.Value\n");
    result.push_str("encodeError queryId message =\n");
    result.push_str("    Encode.object\n");
    result.push_str("        [ ( \"queryId\", Encode.string queryId )\n");
    result.push_str("        , ( \"message\", Encode.string message )\n");
    result.push_str("        ]\n");

    result
}
