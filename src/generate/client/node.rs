use crate::ast;
use crate::ext::string;
use crate::filesystem;
use crate::generate::typealias;

use crate::typecheck;
use std::path::Path;

pub fn generate(
    context: &typecheck::Context,
    base_path: &Path,
    database: &ast::Database,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    // Generate schema metadata file
    files.push(filesystem::GeneratedFile {
        path: base_path.join("schema.ts"),
        contents: to_schema_metadata(context),
    });
}

fn to_node_formatter() -> typealias::TypeFormatter {
    typealias::TypeFormatter {
        to_comment: Box::new(|s| format!("// {}\n", s)),
        to_type_def_start: Box::new(|name| format!("export const {} = Ark.type({{\n", name)),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
                 is_array_relationship,
             }| {
                let (base_type, is_primitive, needs_coercion) = match type_ {
                    "String" => ("string".to_string(), true, false),
                    "Int" => ("number".to_string(), true, false),
                    "Float" => ("number".to_string(), true, false),
                    "Bool" => ("boolean".to_string(), true, true),
                    "DateTime" => ("Date".to_string(), true, true),
                    _ => {
                        if is_link {
                            (type_.to_string(), false, false)
                        } else {
                            (format!("Decode.{}", type_.to_string()), false, false)
                        }
                    }
                };

                // Generate validator with coercion for Date and Bool types
                let type_str = if needs_coercion {
                    match type_ {
                        "DateTime" => {
                            // Reference the CoercedDate helper type
                            match (is_link, is_array_relationship, is_optional) {
                                (true, true, _) => "CoercedDate.array()".to_string(), // One-to-many: array (not optional)
                                (true, false, true) => "CoercedDate.or('null')".to_string(), // Many-to-one/one-to-one: nullable object (can be null)
                                (true, false, false) => "CoercedDate".to_string(), // Many-to-one/one-to-one: required object (shouldn't happen but handle it)
                                (false, _, true) => "CoercedDate.or('null')".to_string(), // Nullable single
                                (false, _, false) => "CoercedDate".to_string(), // Required single
                            }
                        }
                        "Bool" => {
                            // Reference the CoercedBool helper type
                            match (is_link, is_array_relationship, is_optional) {
                                (true, true, _) => "CoercedBool.array()".to_string(), // One-to-many: array (not optional)
                                (true, false, true) => "CoercedBool.or('null')".to_string(), // Many-to-one/one-to-one: nullable object (can be null)
                                (true, false, false) => "CoercedBool".to_string(), // Many-to-one/one-to-one: required object (shouldn't happen but handle it)
                                (false, _, true) => "CoercedBool.or('null')".to_string(), // Nullable single
                                (false, _, false) => "CoercedBool".to_string(), // Required single
                            }
                        }
                        _ => unreachable!(),
                    }
                } else {
                    // Standard type handling without coercion
                    match (is_primitive, is_link, is_array_relationship, is_optional) {
                        // Primitive types
                        (true, true, true, _) => format!("\"{}[]\"", base_type), // One-to-many: array
                        (true, true, false, true) => format!("\"{} | null\"", base_type), // Many-to-one/one-to-one: nullable
                        (true, true, false, false) => format!("\"{}\"", base_type), // Many-to-one/one-to-one: required (shouldn't happen)
                        (true, false, _, true) => format!("\"{} | null\"", base_type),
                        (true, false, _, false) => format!("\"{}\"", base_type),
                        // Non-primitive types
                        (false, true, true, _) => format!("{}.array()", base_type), // One-to-many: array (not optional)
                        (false, true, false, true) => format!("{}.or('null')", base_type), // Many-to-one/one-to-one: nullable object (can be null)
                        (false, true, false, false) => base_type.to_string(), // Many-to-one/one-to-one: required object (shouldn't happen)
                        (false, false, _, true) => format!("{}.or('null')", base_type),
                        (false, false, _, false) => base_type.to_string(),
                    }
                };
                format!("  {}: {}", name, type_str)
            },
        ),
        to_type_def_end: Box::new(|| "});\n".to_string()),
        to_field_separator: Box::new(|is_last| {
            if is_last {
                "\n".to_string()
            } else {
                ",\n".to_string()
            }
        }),
    }
}

// pub fn write_schema(database: &ast::Database) -> String {
//     let mut result = String::new();

//     result.push_str("module Db exposing (..)\n\nimport Time\n\n\n");

//     result.push_str("type alias DateTime =\n    Time.Posix\n\n\n");

//     for schema in &database.schemas {
//         for file in &schema.files {
//             for definition in &file.definitions {
//                 result.push_str(&to_string_definition(definition));
//             }
//         }
//     }

//     result
// }

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
    let type_str = column.type_.to_string();
    if is_first {
        return format!("{} : {}{}\n", column.name, maybe, type_str);
    } else {
        let spaces = " ".repeat(indent);
        return format!("{}, {} : {}{}\n", spaces, column.name, maybe, type_str);
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
            if *count > 2 {
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

fn to_json_type_decoder(type_: &ast::ColumnType) -> String {
    match type_ {
        ast::ColumnType::String => "Decode.string".to_string(),
        ast::ColumnType::Int => "Decode.int".to_string(),
        ast::ColumnType::Float => "Decode.float".to_string(),
        ast::ColumnType::DateTime => "Db.Read.dateTime".to_string(),
        _ => crate::ext::string::decapitalize(&type_.to_string()).to_string(),
    }
}

fn to_type_decoder(column: &ast::Column) -> String {
    let decoder = match &column.type_ {
        ast::ColumnType::String => "Db.Read.string".to_string(),
        ast::ColumnType::Int => "Db.Read.int".to_string(),
        ast::ColumnType::Float => "Db.Read.float".to_string(),
        ast::ColumnType::DateTime => "Db.Read.dateTime".to_string(),
        ast::ColumnType::Bool => "Db.Read.bool".to_string(),
        _ => format!(
            "Db.Decode.{}",
            crate::ext::string::decapitalize(&column.type_.to_string())
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

fn to_type_encoder(_fieldname: &str, type_: &ast::ColumnType) -> String {
    match type_ {
        ast::ColumnType::String => "Encode.string".to_string(),
        ast::ColumnType::Int => "Encode.int".to_string(),
        ast::ColumnType::Float => "Encode.float".to_string(),
        ast::ColumnType::DateTime => "Db.Encode.dateTime".to_string(),
        _ => format!("Db.Encode.{}", string::decapitalize(&type_.to_string())).to_string(),
    }
}

fn to_type_encoder_str(_fieldname: &str, type_: &str) -> String {
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
    dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    let formatter = to_node_formatter();

    // Generate individual query files
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                files.push(filesystem::GeneratedFile {
                    path: dir.join("query").join(format!("{}.ts", q.name.to_string())),
                    contents: to_query_file(context, q, &formatter),
                });
            }
            _ => continue,
        }
    }

    // Generate queries.ts file that collects all queries for easy importing
    files.push(filesystem::GeneratedFile {
        path: dir.join("queries.ts"),
        contents: generate_queries_collection(&query_list),
    });
}

fn generate_queries_collection(query_list: &ast::QueryList) -> String {
    let mut content = String::new();

    content.push_str("// Auto-generated file: collects all query modules for easy importing\n");
    content.push_str("// This file is regenerated when queries are generated\n\n");
    content.push_str("import type { QueryModule } from '@pyre/client/elm-adapter';\n\n");

    // Import all query modules
    let mut query_names: Vec<String> = Vec::new();
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let query_name = q.name.to_string();
                query_names.push(query_name.clone());
                content.push_str(&format!(
                    "import {{ {} }} from './query/{}';\n",
                    query_name,
                    crate::ext::string::decapitalize(&query_name)
                ));
            }
            _ => continue,
        }
    }

    if query_names.is_empty() {
        content.push_str("\nexport const queries: Record<string, QueryModule> = {};\n");
        return content;
    }

    // Generate the queries object
    content.push_str("\nexport const queries: Record<string, QueryModule> = {\n");
    let mut is_first = true;
    for query_name in &query_names {
        if !is_first {
            content.push_str(",\n");
        }
        content.push_str(&format!("  {}: {}", query_name, query_name));
        is_first = false;
    }
    content.push_str("\n};\n");

    content
}

fn to_query_file(
    context: &typecheck::Context,
    query: &ast::Query,
    formatter: &typealias::TypeFormatter,
) -> String {
    let mut result = String::new();
    result.push_str("import * as Ark from 'arktype';\n");
    result.push_str("import type { QueryShape } from '@pyre/client';\n\n");

    // Generate helper types for coercion (Date and Bool)
    result.push_str("// Coercion helpers for IndexedDB data (numbers -> Date/boolean)\n");
    result
        .push_str("const CoercedDate = Ark.type('number').pipe((val) => new Date(val * 1000));\n");
    result.push_str("const CoercedBool = Ark.type('number').pipe((val) => val !== 0);\n\n");

    // Operation type
    let operation_str = match query.operation {
        ast::QueryOperation::Query => "query",
        ast::QueryOperation::Insert => "insert",
        ast::QueryOperation::Update => "update",
        ast::QueryOperation::Delete => "delete",
    };
    result.push_str(&format!(
        "export const operation = \"{}\" as const;\n\n",
        operation_str
    ));

    result.push_str(&format!(
        "export const hash = \"{}\"\n\n",
        &query.interface_hash
    ));

    result.push_str(&to_param_type_alias(&query.args));

    result.push_str("\n\n");

    // Generate QueryShape (only for queries, not mutations)
    if query.operation == ast::QueryOperation::Query {
        result.push_str(&to_query_shape(context, query));
        result.push_str("\n\n");
    }

    typealias::return_data_aliases(context, query, &mut result, formatter);

    // Export the query module as a named export for easy importing
    result.push_str(&format!("\n\nexport const {} = {{\n", query.name));
    result.push_str("  operation,\n");
    result.push_str("  hash,\n");
    result.push_str("  InputValidator,\n");
    result.push_str("  ReturnData,\n");
    if query.operation == ast::QueryOperation::Query {
        result.push_str("  queryShape,\n");
        result.push_str("  toQueryShape: (input: Input) => queryShape,\n");
    }
    result.push_str("};\n");

    result
}

fn to_query_shape(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = "export const queryShape: QueryShape = {\n".to_string();

    let mut is_first_table = true;
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                if !is_first_table {
                    result.push_str(",\n");
                }
                is_first_table = false;

                // Use query field name (aliased name) instead of table name
                // This matches what's in the ReturnData and what users write in queries
                let field_name = ast::get_aliased_name(query_field);

                result.push_str(&format!("  {}: {{\n", string::quote(&field_name)));

                // Convert query fields to QueryShape format
                result.push_str(&to_query_field_spec(context, query_field));

                result.push_str("\n  }");
            }
            _ => {}
        }
    }

    result.push_str("\n};\n");
    result
}

fn to_query_field_spec(context: &typecheck::Context, query_field: &ast::QueryField) -> String {
    let mut result = String::new();
    let mut is_first = true;

    // Get table info for relationship detection
    let table = context.tables.get(&query_field.name);

    // Extract special directives (@where, @sort, @limit)
    let mut sort_clauses: Vec<String> = Vec::new();
    let mut limit: Option<i32> = None;

    // Collect all field selections and args
    let mut field_selections: Vec<(String, bool, bool)> = Vec::new(); // (name, is_relationship, has_nested_fields)

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
                            "{{ field: {}, direction: {} }}",
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
                // Check if this is a relationship field
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
            result.push_str(",\n");
        }
        is_first = false;

        if is_relationship && has_nested_fields {
            // Relationship field with nested selections - recurse
            // Find the nested field to recurse on
            if let Some(nested_field) = query_field.fields.iter().find_map(|f| match f {
                ast::ArgField::Field(qf) if qf.name == field_name => Some(qf),
                _ => None,
            }) {
                result.push_str(&format!("    {}: {{\n", string::quote(&field_name)));
                result.push_str(&to_query_field_spec(context, nested_field));
                result.push_str("\n    }");
            }
        } else if is_relationship {
            // Relationship field without nested selections - just true
            result.push_str(&format!("    {}: true", string::quote(&field_name)));
        } else {
            // Regular field - just true
            result.push_str(&format!("    {}: true", string::quote(&field_name)));
        }
    }

    // Add special directives if present
    if !sort_clauses.is_empty() || limit.is_some() {
        if !is_first {
            result.push_str(",\n");
        }

        if !sort_clauses.is_empty() {
            if sort_clauses.len() == 1 {
                result.push_str(&format!("    '@sort': {}", sort_clauses[0]));
            } else {
                result.push_str(&format!("    '@sort': [{}]", sort_clauses.join(", ")));
            }
        }

        if let Some(limit_val) = limit {
            if !sort_clauses.is_empty() {
                result.push_str(",\n");
            }
            result.push_str(&format!("    '@limit': {}", limit_val));
        }
    }

    result
}

fn to_schema_metadata(context: &typecheck::Context) -> String {
    let mut result = String::new();
    result.push_str("export interface LinkInfo {\n");
    result.push_str("  type: 'many-to-one' | 'one-to-many' | 'one-to-one';\n");
    result.push_str("  from: string;\n");
    result.push_str("  to: {\n");
    result.push_str("    table: string;\n");
    result.push_str("    column: string;\n");
    result.push_str("  };\n");
    result.push_str("}\n\n");

    result.push_str("export interface IndexInfo {\n");
    result.push_str("  field: string;\n");
    result.push_str("  unique: boolean;\n");
    result.push_str("  primary: boolean;\n");
    result.push_str("}\n\n");

    result.push_str("export interface TableMetadata {\n");
    result.push_str("  name: string;\n");
    result.push_str("  links: Record<string, LinkInfo>;\n");
    result.push_str("  indices: IndexInfo[];\n");
    result.push_str("}\n\n");

    result.push_str("export interface SchemaMetadata {\n");
    result.push_str("  tables: Record<string, TableMetadata>;\n");
    result.push_str("  queryFieldToTable: Record<string, string>;\n");
    result.push_str("}\n\n");

    result.push_str("export const schemaMetadata: SchemaMetadata = {\n");
    result.push_str("  tables: {\n");

    let mut is_first_table = true;
    for (_record_name, table) in &context.tables {
        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);

        if !is_first_table {
            result.push_str(",\n");
        }
        is_first_table = false;

        result.push_str(&format!("    {}: {{\n", string::quote(&table_name)));
        result.push_str(&format!("      name: {},\n", string::quote(&table_name)));
        result.push_str("      links: {\n");

        // Get all links from this table
        let links = ast::collect_links(&table.record.fields);
        let primary_key_name = ast::get_primary_id_field_name(&table.record.fields);

        let mut is_first_rel = true;
        for link in links {
            if !is_first_rel {
                result.push_str(",\n");
            }
            is_first_rel = false;

            // Determine link type
            let is_many_to_one = link
                .local_ids
                .iter()
                .any(|id| primary_key_name.as_ref().map(|pk| id != pk).unwrap_or(true));

            // Get foreign table to check for one-to-one and get table name
            // get_linked_table should always succeed for valid schemas - if it returns None, that's a bug
            let foreign_table = get_linked_table(context, &link).expect(&format!(
                "Failed to find linked table '{}' in context. This indicates a schema error.",
                link.foreign.table
            ));
            let foreign_table_name =
                ast::get_tablename(&foreign_table.record.name, &foreign_table.record.fields);

            let is_one_to_one = if is_many_to_one {
                // For one-to-one, both conditions must be true:
                // 1. The foreign field being linked to must be unique (primary key or unique constraint)
                // 2. The local foreign key field must also be unique (so only one row can reference a given foreign row)
                let foreign_field_is_unique =
                    ast::linked_to_unique_field_with_record(&link, &foreign_table.record);
                let local_field_is_unique = if link.local_ids.len() == 1 {
                    ast::field_is_unique(&link.local_ids[0], &table.record)
                } else {
                    false // Multi-field unique constraints not yet supported
                };
                foreign_field_is_unique && local_field_is_unique
            } else {
                false
            };

            let link_type = if is_one_to_one {
                "one-to-one"
            } else if is_many_to_one {
                "many-to-one"
            } else {
                "one-to-many"
            };

            // Determine "from" (local column) and "to" (foreign table and column)
            let (from_column, to_table, to_column) = if is_many_to_one {
                // For many-to-one/one-to-one: FK is in current table (local_ids), points to foreign table
                (
                    link.local_ids[0].clone(),
                    foreign_table_name,
                    link.foreign.fields[0].clone(),
                )
            } else {
                // For one-to-many: FK is in foreign table (foreign.fields), points to current table's PK
                (
                    primary_key_name.clone().unwrap_or_else(|| "id".to_string()),
                    foreign_table_name,
                    link.foreign.fields[0].clone(),
                )
            };

            result.push_str(&format!("        {}: {{\n", string::quote(&link.link_name)));
            result.push_str(&format!("          type: {},\n", string::quote(link_type)));
            result.push_str(&format!(
                "          from: {},\n",
                string::quote(&from_column)
            ));
            result.push_str("          to: {\n");
            result.push_str(&format!(
                "            table: {},\n",
                string::quote(&to_table)
            ));
            result.push_str(&format!(
                "            column: {}\n",
                string::quote(&to_column)
            ));
            result.push_str("          }\n");
            result.push_str("        }");
        }

        result.push_str("\n      },\n");

        result.push_str("      indices: [\n");

        let mut is_first_index = true;
        for field in &table.record.fields {
            if let ast::Field::Column(column) = field {
                let is_primary = ast::is_primary_key(column);
                let is_unique = column
                    .directives
                    .iter()
                    .any(|d| matches!(d, ast::ColumnDirective::Unique));
                let is_index = column
                    .directives
                    .iter()
                    .any(|d| matches!(d, ast::ColumnDirective::Index));

                if is_primary || is_unique || is_index {
                    if !is_first_index {
                        result.push_str(",\n");
                    }
                    is_first_index = false;

                    result.push_str("        {\n");
                    result.push_str(&format!(
                        "          field: {},\n",
                        string::quote(&column.name)
                    ));
                    result.push_str(&format!(
                        "          unique: {},\n",
                        if is_unique || is_primary {
                            "true"
                        } else {
                            "false"
                        }
                    ));
                    result.push_str(&format!(
                        "          primary: {}\n",
                        if is_primary { "true" } else { "false" }
                    ));
                    result.push_str("        }");
                }
            }
        }

        result.push_str("\n      ]\n");
        result.push_str("    }");
    }

    result.push_str("\n  },\n");
    result.push_str("  queryFieldToTable: {\n");

    // Generate mapping from query field names (lowercase record names) to table names
    let mut is_first_mapping = true;
    for (_record_name, table) in &context.tables {
        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
        let query_field_name = crate::ext::string::decapitalize(&table.record.name);

        if !is_first_mapping {
            result.push_str(",\n");
        }
        is_first_mapping = false;

        result.push_str(&format!(
            "    {}: {}",
            string::quote(&query_field_name),
            string::quote(&table_name)
        ));
    }

    result.push_str("\n  }\n");
    result.push_str("};\n");
    result
}

fn get_linked_table<'a>(
    context: &'a typecheck::Context,
    link: &ast::LinkDetails,
) -> Option<&'a typecheck::Table> {
    // context.tables is keyed by decapitalized record names, so we need to decapitalize when looking up
    context
        .tables
        .get(&crate::ext::string::decapitalize(&link.foreign.table))
}

fn to_arktype_type(type_: &str) -> String {
    match type_ {
        "String" => "\"string\"".to_string(),
        "Int" => "\"number\"".to_string(),
        "Bool" => "\"boolean\"".to_string(),
        "DateTime" => "\"date\"".to_string(),
        _ => format!("\"{}\"", type_),
    }
}

fn to_param_type_alias(args: &Vec<ast::QueryParamDefinition>) -> String {
    let mut result = "export const InputValidator = Ark.type({".to_string();
    let mut is_first = true;
    for arg in args {
        let type_string = to_arktype_type(&arg.type_.clone().unwrap_or("unknown".to_string()));
        if is_first {
            result.push_str(&format!("\n  {}: {}", arg.name, type_string));
            is_first = false;
        } else {
            result.push_str(&format!(",\n  {}: {}", arg.name, type_string));
        }
    }
    result.push_str("\n});\n\n\n");

    result.push_str("export type Input = typeof InputValidator.infer\n");
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
                to_type_encoder_str(&arg.name, &type_string),
                &arg.name
            ));
            is_first = false;
        } else {
            result.push_str(&format!(
                "        , ( {}, {} input.{})\n",
                string::quote(&arg.name),
                to_type_encoder_str(&arg.name, &type_string),
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
                ast::get_select_alias(table_alias, query_field),
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

fn to_elm_typename(type_: &ast::ColumnType) -> String {
    match type_ {
        ast::ColumnType::String => "String".to_string(),
        ast::ColumnType::Int => "Int".to_string(),
        ast::ColumnType::Float => "Float".to_string(),
        ast::ColumnType::Bool => "Bool".to_string(),
        ast::ColumnType::DateTime => "Time.Posix".to_string(),
        _ => format!("Db.{}", type_.to_string()).to_string(),
    }
}
