pub mod watched;
use crate::ast;
use crate::ext::string;
use crate::filesystem;
use crate::generate;
use crate::generate::typealias;
use crate::typecheck;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

const DB_ENGINE: &str = include_str!("./static/typescript/db.ts");

/// Write all typescript files
pub fn write(
    context: &typecheck::Context,
    database: &ast::Database,
    typescript_dir: &Path,
) -> io::Result<()> {
    filesystem::create_dir_if_not_exists(&typescript_dir)?;
    filesystem::create_dir_if_not_exists(&typescript_dir.join("db"))?;

    watched::write(&typescript_dir, context);

    // Top level TS files
    // DB engine as db.ts
    let ts_db_path = &typescript_dir.join(Path::new("db.ts"));
    let ts_file = Path::new(&ts_db_path);
    let mut output = fs::File::create(ts_file).expect("Failed to create file");
    output
        .write_all(DB_ENGINE.as_bytes())
        .expect("Failed to write to file");

    // Config types as db/env.ts
    // Includes:
    //   Session type
    //   Database mapping
    let ts_config_path_str = &typescript_dir.join(Path::new("db/env.ts"));
    let ts_config_path = Path::new(&ts_config_path_str);
    let mut output = fs::File::create(ts_config_path).expect("Failed to create file");
    if let Some(config_ts) = to_env(&database) {
        output
            .write_all(config_ts.as_bytes())
            .expect("Failed to write to file");
    }

    // Schema-level data types
    let ts_db_data_path = &typescript_dir.join(Path::new("db/data.ts"));
    let ts_data_path = Path::new(&ts_db_data_path);
    let mut output_data = fs::File::create(ts_data_path).expect("Failed to create file");
    let formatted_ts = schema(&database);
    output_data
        .write_all(formatted_ts.as_bytes())
        .expect("Failed to write to file");

    // TS Decoders for custom types.
    let ts_db_decoder_path = &typescript_dir.join(Path::new("db/decode.ts"));
    let ts_decoders = to_schema_decoders(&database);
    let ts_decoder_file = Path::new(&ts_db_decoder_path);
    let mut output = fs::File::create(ts_decoder_file).expect("Failed to create file");
    output
        .write_all(ts_decoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

pub fn schema(database: &ast::Database) -> String {
    let mut result = String::new();

    for schema in &database.schemas {
        result.push_str("\n\n");
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
        ast::Definition::Comment { .. } => "".to_string(),
        ast::Definition::Session(_) => "".to_string(),
        ast::Definition::Tagged { name, variants, .. } => {
            let mut result = format!("type {} =", name);
            
            for variant in variants {
                result.push_str("\n");
                result.push_str(&to_string_variant( 2, variant));
            }
            result.push_str(";\n\n");
            result
        }
        ast::Definition::Record { name, fields, .. } => to_type_alias(name, fields),
    }
}

fn to_type_alias(name: &str, fields: &Vec<ast::Field>) -> String {
    let mut result = format!("type {} = {{\n  ", name);

    let mut is_first = true;
    for field in fields {
        if ast::is_column_space(field) {
            continue;
        }

        result.push_str(&to_string_field(is_first, 2, &field));

        if is_first & ast::is_column(field) {
            is_first = false;
        }
    }
    result.push_str("};\n");
    result
}

fn to_string_variant( indent_size: usize, variant: &ast::Variant) -> String {
    let prefix = " | ";

    match &variant.fields {
        Some(fields) => {
            let indent = " ".repeat(indent_size + 4);

            let mut result = format!(
                " {}{{\n{}\"type\": {};\n{}",
                prefix,
                indent,
                crate::ext::string::quote(&variant.name),
                indent
            );

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_string_field(is_first_field, indent_size + 4, &field));
                is_first_field = false
            }
            result.push_str("    }");
            result
        }
        None => format!(
            " {}{{ \"type\": {} }}",
            prefix,
            crate::ext::string::quote(&variant.name)
        ),
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
    if is_first {
        return format!(
            "{}: {};\n",
            crate::ext::string::quote(&column.name),
            to_ts_typename(false, &column.type_)
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}{}: {};\n",
            spaces,
            crate::ext::string::quote(&column.name),
            to_ts_typename(false, &column.type_)
        );
    }
}

// DECODE
//

pub fn to_schema_decoders(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("import * as Ark from 'arktype';");

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
        ast::Definition::Comment { .. } => "".to_string(),
        ast::Definition::Session(_) => "".to_string(),
        ast::Definition::Tagged { name, variants, .. } => {
            let mut result = "".to_string();

            result.push_str(&format!("export const {} = ", name));
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_decoder_variant(is_first, 2, name, variant));
                is_first = false;
            }
            result.push_str("\n");
            // result.push_str("        |> Db.Read.custom\n");
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

    let or = &format!("{}{}", outer_indent, ".or");
    let starter = if is_first { "Ark.type" } else { or };

    match &variant.fields {
        Some(fields) => {
            let mut result = format!(
                "{}({{\n    \"type_\": {},\n",
                starter,
                crate::ext::string::quote(&crate::ext::string::single_quote(&variant.name)),
            );

            for field in fields {
                result.push_str(&to_variant_field_json_decoder(indent_size + 2, &field));
            }
            result.push_str(&format!("{}}})\n", outer_indent));

            result
        }
        None => format!(
            "{}({{ \"type_\": {} }})\n",
            starter,
            crate::ext::string::quote(&crate::ext::string::single_quote(&variant.name)),
        ),
    }
}

// Field directives(specifically @link) is not allowed within a type at the moment
fn to_variant_field_json_decoder(indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}{}: {},\n",
                spaces,
                crate::ext::string::quote(&column.name),
                to_ts_type_decoder(true, column.nullable, &column.type_)
            );
        }
        _ => "".to_string(),
    }
}


//  QUERIES
//
pub fn write_queries(
    dir: &Path,
    context: &typecheck::Context,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    query_list: &ast::QueryList,
) -> io::Result<()> {
    let query_dir = dir.join("query");
    filesystem::create_dir_if_not_exists(&query_dir)?;
    write_runner(dir, context, query_list);

    let formatter = to_formatter();

    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let query_info = all_query_info.get(&q.name).unwrap();

                let target_path =
                    query_dir.join(&format!("{}.ts", crate::ext::string::decapitalize(&q.name)));

                let mut output = fs::File::create(target_path).expect("Failed to create file");
                output
                    .write_all(to_query_file(&context, &query_info, &q, &formatter).as_bytes())
                    .expect("Failed to write to file");
            }
            _ => continue,
        }
    }
    Ok(())
}

fn write_runner(dir: &Path, context: &typecheck::Context, query_list: &ast::QueryList) {
    let target_path = dir.join("query.ts");
    let mut content = String::new();

    content.push_str("import * as Env from \"./db/env\";\n");
    content.push_str("import * as Db from \"./db\";\n");
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                content.push_str(&format!(
                    "import * as {} from './query/{}';\n",
                    q.name,
                    crate::ext::string::decapitalize(&q.name)
                ));
            }
            _ => continue,
        }
    }
    content.push_str("\nexport const run = async (\n");
    content.push_str("  env: Env.Config,\n");
    content.push_str("  id: string,\n");
    content.push_str("  session: Env.Session,\n");
    content.push_str("  args: any,\n");
    content.push_str("): Promise<Db.ExecuteResult> => {\n");
    content.push_str("    switch (id) {\n");

    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                content.push_str(&format!("        case \"{}\":\n", q.interface_hash));

                content.push_str(&format!(
                    "            return {}.query.run(env, session, args);\n",
                    &q.name
                ));
            }
            _ => continue,
        }
    }
    content.push_str("        default:\n");
    content.push_str(
        "            return { kind: \"error\", errorType: Db.ErrorType.UnknownQuery, message: \"\" }\n"
    );

    content.push_str("    }\n");
    content.push_str("};\n");

    let mut output = fs::File::create(target_path).expect("Failed to create file");
    output
        .write_all(content.as_bytes())
        .expect("Failed to write to file");
}

pub fn literal_quote(s: &str) -> String {
    format!("`{}`", s)
}

fn format_ts_list(items: Vec<String>) -> String {
    let mut result = "[ ".to_string();
    let mut first = true;
    for item in items {
        if first {
            result.push_str(&format!("{}", item));
        } else {
            result.push_str(&format!(", {}", item));
        }

        first = false;
    }
    result.push_str("]");
    result
}

// A specialized
fn get_formatted_used_params(
    top_level_field_alias: &str,
    query_params: &HashMap<String, typecheck::ParamInfo>,
) -> String {
    let mut formatted = String::new();
    formatted.push_str("[ ");
    let mut first_added = false;
    for (_, param_info) in query_params {
        match param_info {
            typecheck::ParamInfo::NotDefinedButUsed { .. } => continue,
            typecheck::ParamInfo::Defined {
                used_by_top_level_field_alias,
                raw_variable_name,
                ..
            } => {
                if used_by_top_level_field_alias.contains(top_level_field_alias) {
                    if first_added {
                        formatted.push_str(", ")
                    }
                    formatted.push_str(&string::quote(raw_variable_name));

                    first_added = true;
                }
            }
        }
    }
    formatted.push_str(" ]");
    return formatted;
}

fn bool_to_ts_bool(bool: bool) -> String {
    if bool {
        return "true".to_string();
    }
    return "false".to_string();
}

pub fn to_formatter() -> typealias::TypeFormatter {
    typealias::TypeFormatter {
        to_comment: Box::new(|s| format!("// {}\n", s)),
        to_type_def_start: Box::new(|name| format!("export const {} = Ark.type({{\n", name)),
        to_field: Box::new(
            |name,
             type_,
             typealias::FieldMetadata {
                 is_link,
                 is_optional,
             }| {
                let (base_type, is_primitive) = match type_ {
                    "String" => ("string".to_string(), true),
                    "Int" => ("number".to_string(), true),
                    "Float" => ("number".to_string(), true),
                    "Bool" => ("boolean".to_string(), true),
                    "DateTime" => ("Date".to_string(), true),
                    _ => {
                        if is_link {
                            (type_.to_string(), false)
                        } else {
                            (format!("Decode.{}", type_.to_string()), false)
                        }
                    }
                };

                #[rustfmt::skip]
                let type_str = match (is_primitive, is_link, is_optional) {
                    // Primitive types
                    (true, true, true) => format!("\"{}?\"", base_type),
                    (true, true, false) => format!("\"{}[]\"", base_type),
                    (true, false, true) => format!("\"{}?\"", base_type),
                    (true, false, false) => format!("\"{}\"", base_type),
                    
                    // Non-primitive types
                    (false, true, true) => format!("\"{}?\"", base_type),
                    (false, true, false) => format!("[{}]", base_type), 
                    (false, false, true) => format!("\"{}?\"", base_type),
                    (false, false, false) => base_type.to_string()
                };
                format!("    {}: {}", name, type_str)
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

fn to_query_file(
    context: &typecheck::Context,
    query_info: &typecheck::QueryInfo,
    query: &ast::Query,
    formatter: &typealias::TypeFormatter,
) -> String {
    let mut result = "".to_string();
    result.push_str("import * as Ark from 'arktype';\n");
    result.push_str("import * as Db from '../db';\n");
    result.push_str("import * as Watched from '../watched';\n");
    result.push_str("import * as Decode from '../db/decode';\n");
    result.push_str("import * as Env from '../db/env';\n\n");

    // Input args decoder
    to_query_input_decoder(context, &query, &mut result);

    result.push_str("\n\nconst sql = [");
    let mut watchers = vec![];

    let mut written_field = false;
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let table = context.tables.get(&query_field.name).unwrap();

                for watched_operation in ast::to_watched_operations(&table.record) {
                    let name = format!(
                        "{}{}",
                        table.record.name,
                        watched::operation_name(&watched_operation)
                    );
                    watchers.push(format!(
                        "{{ kind: Watched.WatchedKind.{}, data: {{}} }}",
                        name
                    ));
                }

                let param_info = get_formatted_used_params(
                    &ast::get_aliased_name(query_field),
                    &query_info.variables,
                );
                let prepared =
                    generate::sql::to_string(context, query, query_info, table, query_field);

                for prepped in prepared {
                    if written_field {
                        result.push_str(",\n");
                    }
                    result.push_str(&format!(
                        "{{\n  include: {},\n  params: {},\n  sql: {}}}",
                        bool_to_ts_bool(prepped.include),
                        &param_info,
                        &literal_quote(&prepped.sql)
                    ));
                    written_field = true;
                }
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

    result.push_str("];\n\n\n");

    // return_data_aliases(context, query, &mut result);

    typealias::return_data_aliases(context, query, &mut result, formatter);

    // Rectangle data decoder
    // result.push_str("export const ReturnRectangle = Ark.type({\n");
    // for field in &query.fields {
    //     match field {
    //         ast::TopLevelQueryField::Field(query_field) => {
    //             let table = context.tables.get(&query_field.name).unwrap();

    //             to_flat_query_decoder(
    //                 context,
    //                 &ast::get_aliased_name(&query_field),
    //                 &table.record,
    //                 &ast::collect_query_fields(&query_field.fields),
    //                 &mut result,
    //             );
    //         }
    //         ast::TopLevelQueryField::Lines { .. } => {}
    //         ast::TopLevelQueryField::Comment { .. } => {}
    //     }
    // }
    // result.push_str("});\n\n");

    let session_args = get_session_args(&query_info.variables);

    let validate = format!(
        r#"
export const query = Db.to_runner({{
    id: "{}",
    primary_db: Env.DatabaseKey.{},
    attached_dbs: {},
    sql: sql,
    session: Env.Session,
    session_args: {},
    input: Input,
    output: ReturnData,
    watch_triggers: {}
}});

type Input = typeof Input.infer
"#,
        query.interface_hash,
        query_info.primary_db,
        format!(
            "[{}]",
            query_info
                .attached_dbs
                .iter()
                .map(|db| format!("Env.DatabaseKey.{}", db))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        session_args,
        format_ts_list(watchers)
    );

    result.push_str(&validate);

    // // Type Alisaes
    // result.push_str("// Return data\n");
    // for field in &query.fields {
    //     let table = context.tables.get(&field.name).unwrap();
    //     result.push_str(&to_query_type_alias(
    //         context,
    //         table,
    //         &field.name,
    //         &ast::collect_query_fields(&field.fields),
    //     ));
    // }

    // TODO:: HTTP Sender

    // Nested Return data decoders
    // result.push_str("\n\n");
    // for field in &query.fields {
    //     let table = context.tables.get(&field.name).unwrap();
    //     result.push_str(&to_query_decoder(
    //         context,
    //         &ast::get_aliased_name(&field),
    //         table,
    //         &ast::collect_query_fields(&field.fields),
    //     ));
    // }
    //

    result
}



fn get_session_args(params: &HashMap<String, typecheck::ParamInfo>) -> String {
    let mut result = "[ ".to_string();
    let mut first = true;
    for (_name, info) in params {
        match info {
            typecheck::ParamInfo::Defined {
                from_session,
                used,
                session_name,
                ..
            } => {
                if *from_session && *used {
                    match session_name {
                        None => continue,
                        Some(session_name_string) => {
                            if first {
                                result.push_str(&format!(
                                    "{}",
                                    crate::ext::string::quote(session_name_string)
                                ));
                            } else {
                                result.push_str(&format!(
                                    ", {}",
                                    crate::ext::string::quote(session_name_string)
                                ));
                            }
                            first = false;
                        }
                    }
                }
            }
            _ => continue,
        }
    }
    result.push_str("]");
    result
}

fn to_query_input_decoder(_context: &typecheck::Context, query: &ast::Query, result: &mut String) {
    result.push_str("export const Input = Ark.type({");
    for arg in &query.args {
        result.push_str(&format!(
            "\n  {}: {},",
            crate::ext::string::quote(&arg.name),
            to_ts_type_decoder(
                true,
                false,
                &arg.type_.clone().unwrap_or("unknown".to_string())
            )
        ));
    }
    result.push_str("\n});\n");
}

fn to_flat_query_decoder(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        to_table_field_flat_decoder(2, context, table_alias, table_field, field, result)
    }
}

fn to_table_field_flat_decoder(
    indent: usize,
    context: &typecheck::Context,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
    result: &mut String,
) {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            result.push_str(&format!(
                "{}\"{}\": {},\n",
                spaces,
                ast::get_select_alias(table_alias, query_field),
                to_ts_type_decoder(true, column.nullable, &column.type_)
            ));
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let table = typecheck::get_linked_table(context, &link).unwrap();

            to_flat_query_decoder(
                context,
                &ast::get_aliased_name(&query_field),
                &table.record,
                &ast::collect_query_fields(&query_field.fields),
                result,
            )
        }

        _ => (),
    }
}

fn to_ts_typename(qualified: bool, type_: &str) -> String {
    match type_ {
        "String" => "string".to_string(),
        "Int" => "number".to_string(),
        "Float" => "number".to_string(),
        "Bool" => "boolean".to_string(),
        _ => {
            let qualification = if qualified { "Db." } else { "" };
            return format!("{}{}", qualification, type_).to_string();
        }
    }
}

fn to_nullable_type(is_nullable: bool, type_: &str) -> String {
    if is_nullable {
        format!("{} | null", type_)
    } else {
        type_.to_string()
    }
}

fn to_ts_type_decoder(qualified: bool, nullable: bool, type_: &str) -> String {
    match type_ {
        "String" => crate::ext::string::quote(&to_nullable_type(nullable, "string")),
        "Int" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        "Float" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        "Bool" => crate::ext::string::quote(&to_nullable_type(nullable, "boolean")),
        "DateTime" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        _ => {
            let qualification = if qualified { "Decode." } else { "" };
            return to_nullable_type(nullable, &format!("{}{}", qualification, type_)).to_string();
        }
    }
}


pub fn to_env(database: &ast::Database) -> Option<String> {
    let mut result = String::new();

    result.push_str("import * as Ark from 'arktype';\n");
    let session = database
        .schemas
        .iter()
        .find_map(|schema| schema.session.clone())
        .unwrap_or_else(|| ast::default_session_details());

    if database.schemas.len() == 1 {
        result.push_str("export type Config from '@libsql/client';\n");
    } else {
        result.push_str("import type { Config as LibSqlConfig } from '@libsql/client';\n");
    }

    // Generate session types
    result.push_str("\n\nexport const Session = Ark.type({\n");
    for field in &session.fields {
        match field {
            ast::Field::Column(column) => {
                result.push_str(&format!(
                    "  {}: {},\n",
                    column.name,
                    to_ts_type_decoder(true, column.nullable, &column.type_)
                ));
            }
            _ => (),
        }
    }
    result.push_str("});\n\n");
    result.push_str("export type Session = {\n");
    for field in &session.fields {
        match field {
            ast::Field::Column(column) => {
                result.push_str(&format!(
                    "  {}: {};\n",
                    column.name,
                    to_ts_typename(true, &column.type_)
                ));
            }
            _ => (),
        }
    }
    result.push_str("};\n\n");

    // Database namespaces
    if database.schemas.len() > 1 {
        result.push_str("export interface DatabaseConfig extends LibSqlConfig {\n");
        result.push_str("  /** Unique identifier for the database */\n");
        result.push_str("  id: string;\n");
        result.push_str("}\n");

        result.push_str("export interface Config {\n");
        for schema in &database.schemas {
            result.push_str(&format!("  {}?: DatabaseConfig;\n", schema.namespace));
        }
        result.push_str("}\n\n");
    }

    if database.schemas.len() == 1 {
        result.push_str("export type DatabaseKey = string;\n");
    } else {
        result.push_str("export enum DatabaseKey {\n");
        for schema in &database.schemas {
            result.push_str(&format!(
                "  {} = '{}',\n",
                schema.namespace, schema.namespace
            ));
        }
        result.push_str("}\n");
    }

    result.push_str(
        "export const to_libSql_config = (env: Config, primary: DatabaseKey): LibSqlConfig | undefined => {\n",
    );
    if database.schemas.len() == 1 {
        result.push_str("  return env")
    } else {
        result.push_str("  return env[primary]")
    }
    result.push_str("\n};\n\n");

    Some(result)
}
