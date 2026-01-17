use crate::ast;
use crate::ext::string;
use crate::filesystem::{generate_text_file, GeneratedFile};

use crate::generate::typealias;
use crate::typecheck;
use std::path::Path;

mod rectangle;

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


{-| Chain field decoders together, similar to Db.Read.field.
This allows you to build up a decoder by adding fields one at a time.

    decodeGame =
        Decode.succeed Game
            |> andField "id" Decode.int 
            |> andField "name" Decode.string

-}
andField : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andField field decoder partial =
    Decode.map2 (\f value -> f value)
        partial
        (Decode.field field decoder)


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

fn to_type_encoder(type_: &str) -> String {
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
    // Collect type names as we generate them
    use std::cell::RefCell;
    use std::rc::Rc;
    
    let type_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    
    // Always include Input
    type_names.borrow_mut().push("Input".to_string());
    
    let mut result = String::new();

    result.push_str("import Db.Decode\n");
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

    // Generate ports and functions - unified for all operations
    // Generate queryShape as JSON encoder (only for queries)
    if query.operation == ast::QueryOperation::Query {
        result.push_str("\n\n");
        result.push_str(&to_query_shape_json(context, query));
        result.push_str("\n\n");
    }
    
    // Generate send command port (pyre_send{capitalized operation})
    let port_name = format!("pyre_send{}", query.name);
    result.push_str(&format!(
        "port {} : Encode.Value -> Cmd msg\n\n",
        port_name
    ));
    
    // Generate send function
    result.push_str("send : Input -> Cmd msg\n");
    result.push_str("send input =\n");
    // All operations just send the encoded input (Elm has already validated)
    result.push_str(&format!("    {} (encode input)\n\n", port_name));
    
    // Generate results subscription port (pyre_receive{capitalized operation})
    let results_port_name = format!("pyre_receive{}", query.name);
    result.push_str(&format!(
        "port {} : (Decode.Value -> msg) -> Sub msg\n\n",
        results_port_name
    ));
    
    // Generate subscription function
    match query.operation {
        ast::QueryOperation::Query => {
            result.push_str("subscription : (ReturnData -> msg) -> Sub msg\n");
            result.push_str("subscription toMsg =\n");
            result.push_str(&format!(
                "    {} (\\json ->\n        case Decode.decodeValue decodeReturnData json of\n            Ok data ->\n                toMsg data\n\n            Err _ ->\n                toMsg (ReturnData [] [])\n    )\n",
                results_port_name
            ));
        }
        _ => {
            result.push_str("subscription : (Result String ReturnData -> msg) -> Sub msg\n");
            result.push_str("subscription toMsg =\n");
            result.push_str(&format!(
                "    {} (\\json ->\n        case Decode.decodeValue decodeReturnData json of\n            Ok data ->\n                toMsg (Ok data)\n\n            Err err ->\n                toMsg (Err (Decode.errorToString err))\n    )\n",
                results_port_name
            ));
        }
    }

    // Build exposing list: functions first, then types
    let mut exposing_items: Vec<String> = Vec::new();
    
    // Add functions first - unified for all operations
    exposing_items.push("send".to_string());
    exposing_items.push("subscription".to_string());
    
    // Then add types (sorted)
    let mut type_names_sorted: Vec<String> = type_names.borrow().clone();
    type_names_sorted.sort();
    exposing_items.extend(type_names_sorted);
    
    // Build the module declaration with explicit exposing
    let exposing_list = exposing_items.join(", ");
    let module_decl = format!("port module Query.{} exposing ({})\n\n\n", query.name, exposing_list);
    
    // Replace the placeholder or prepend the module declaration
    // Since we started with an empty result, we need to prepend
    format!("{}{}", module_decl, result)
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
                to_type_encoder(&type_string),
                &arg.name
            ));
            is_first = false;
        } else {
            result.push_str(&format!(
                "        , ( {}, {} input.{})\n",
                string::quote(&arg.name),
                to_type_encoder(&type_string),
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
                    _ => {}
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
