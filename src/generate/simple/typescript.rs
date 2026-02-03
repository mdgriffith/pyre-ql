use crate::ast;
use crate::ext::string;
use crate::filesystem;
use crate::filesystem::generate_text_file;
use crate::generate;
use crate::generate::sql::to_sql::Prepared;
use crate::generate::typealias;
use crate::typecheck;
use std::collections::HashMap;
use std::path::Path;

/// Generate typesafe standalone TypeScript functions for direct SQLite usage
/// This is for the "simple" use case - no client/server, no sync, just direct DB calls
pub fn generate(
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    // Generate schema types (records, session, custom types)
    files.push(generate_text_file(
        base_out_dir.join("types.ts"),
        generate_schema_types(database),
    ));

    // Generate database client setup
    files.push(generate_text_file(
        base_out_dir.join("db.ts"),
        generate_db_setup(),
    ));
}

/// Generate typesafe query functions
pub fn generate_queries(
    context: &typecheck::Context,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    query_list: &ast::QueryList,
    _database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    // Generate individual query files
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => match all_query_info.get(&q.name) {
                Some(query_info) => {
                    let filename = base_out_dir
                        .join("queries")
                        .join(format!("{}.ts", string::decapitalize(&q.name)));
                    let content = generate_query_function(context, query_info, q);
                    files.push(generate_text_file(filename, content));
                }
                None => {
                    eprintln!(
                        "Warning: Query '{}' was found but not in typecheck results. Skipping.",
                        q.name
                    );
                }
            },
            _ => continue,
        }
    }

    // Generate index.ts that exports all queries
    files.push(generate_text_file(
        base_out_dir.join("index.ts"),
        generate_queries_index(query_list),
    ));
}

fn generate_schema_types(database: &ast::Database) -> String {
    let mut result = String::new();

    result.push_str("// Auto-generated schema types\n");
    result.push_str("import * as Ark from 'arktype';\n\n");
    result
        .push_str("const CoercedDate = Ark.type('number').pipe((val) => new Date(val * 1000));\n");
    result.push_str("const CoercedBool = Ark.type('boolean').or('number').pipe((val) => typeof val === 'number' ? val !== 0 : val);\n\n");

    // Get session definition
    let session = database
        .schemas
        .iter()
        .find_map(|s| s.session.clone())
        .unwrap_or_else(|| ast::default_session_details());

    // Generate Session type
    result.push_str("// Session type\n");
    result.push_str("export interface Session {\n");
    for field in &session.fields {
        if let ast::Field::Column(col) = field {
            let type_str = col.type_.to_string();
            let ts_type = match type_str.as_str() {
                "String" => "string",
                "Int" | "Float" => "number",
                "Bool" => "boolean",
                "DateTime" => "Date",
                other => other,
            };
            let optional = if col.nullable { "?" } else { "" };
            result.push_str(&format!("  {}{}: {};\n", col.name, optional, ts_type));
        }
    }
    result.push_str("}\n\n");

    result.push_str("export const SessionValidator = Ark.type({\n");
    for field in &session.fields {
        if let ast::Field::Column(col) = field {
            let type_str = col.type_.to_string();
            let validator = match type_str.as_str() {
                "String" => "'string'".to_string(),
                "Int" | "Float" => "'number'".to_string(),
                "Bool" => "CoercedBool".to_string(),
                "DateTime" => "CoercedDate".to_string(),
                other => format!("'{}'", other),
            };
            let validator = if col.nullable {
                format!("{}.or('null')", validator)
            } else {
                validator
            };
            result.push_str(&format!("  {}: {},\n", col.name, validator));
        }
    }
    result.push_str("});\n\n");

    // Generate custom type definitions (tagged unions)
    for schema in &database.schemas {
        for file in &schema.files {
            for def in &file.definitions {
                if let ast::Definition::Tagged { name, variants, .. } = def {
                    result.push_str(&generate_tagged_union(name, variants));
                }
            }
        }
    }

    // Generate record types
    for schema in &database.schemas {
        for file in &schema.files {
            for def in &file.definitions {
                if let ast::Definition::Record { name, fields, .. } = def {
                    result.push_str(&generate_record_type(name, fields));
                }
            }
        }
    }

    result
}

fn generate_tagged_union(name: &str, variants: &[ast::Variant]) -> String {
    let mut result = String::new();

    result.push_str(&format!("export type {} =\n", name));
    for (i, variant) in variants.iter().enumerate() {
        let sep = if i == 0 { "  " } else { "| " };
        result.push_str(&format!("{}", sep));

        if let Some(fields) = &variant.fields {
            result.push_str(&format!("{{ type_: '{}'; ", variant.name));
            for field in fields.iter() {
                if let ast::Field::Column(col) = field {
                    let type_str = col.type_.to_string();
                    let ts_type = match type_str.as_str() {
                        "String" => "string",
                        "Int" | "Float" => "number",
                        "Bool" => "boolean",
                        "DateTime" => "Date",
                        other => other,
                    };
                    let optional = if col.nullable { "?" } else { "" };
                    result.push_str(&format!("{}: {}{}; ", col.name, ts_type, optional));
                }
            }
            result.push_str("}}\n");
        } else {
            result.push_str(&format!("{{ type_: '{}' }}\n", variant.name));
        }
    }
    result.push('\n');

    result.push_str(&format!("export const {} = ", name));
    for (i, variant) in variants.iter().enumerate() {
        let prefix = if i == 0 { "Ark.type" } else { ".or" };

        if let Some(fields) = &variant.fields {
            result.push_str(&format!("{}({{\n", prefix));
            result.push_str(&format!(
                "  type_: '{}',\n",
                string::single_quote(&variant.name)
            ));
            for (j, field) in fields.iter().enumerate() {
                if let ast::Field::Column(col) = field {
                    let type_str = col.type_.to_string();
                    let validator = match type_str.as_str() {
                        "String" => "'string'".to_string(),
                        "Int" | "Float" => "'number'".to_string(),
                        "Bool" => "CoercedBool".to_string(),
                        "DateTime" => "CoercedDate".to_string(),
                        other => format!("'{}'", other),
                    };
                    let validator = if col.nullable {
                        format!("{}.or('null')", validator)
                    } else {
                        validator
                    };
                    result.push_str(&format!("  {}: {}", col.name, validator));
                    if j + 1 < fields.len() {
                        result.push_str(",\n");
                    } else {
                        result.push_str("\n");
                    }
                }
            }
            result.push_str("})");
        } else {
            result.push_str(&format!(
                "{}({{ type_: '{}' }})",
                prefix,
                string::single_quote(&variant.name)
            ));
        }
    }
    result.push_str(";\n\n");

    result
}

fn generate_record_type(name: &str, fields: &Vec<ast::Field>) -> String {
    let mut result = String::new();

    result.push_str(&format!("export interface {} {{\n", name));
    for field in fields {
        match field {
            ast::Field::Column(col) => {
                let type_str = col.type_.to_string();
                let ts_type = match type_str.as_str() {
                    "String" => "string",
                    "Int" | "Float" => "number",
                    "Bool" => "boolean",
                    "DateTime" => "Date",
                    other => other,
                };
                let optional = if col.nullable { "?" } else { "" };
                result.push_str(&format!("  {}{}: {};\n", col.name, optional, ts_type));
            }
            _ => {}
        }
    }
    result.push_str("}\n\n");

    result.push_str(&format!("export const {}Validator = Ark.type({{\n", name));
    for field in fields {
        match field {
            ast::Field::Column(col) => {
                let type_str = col.type_.to_string();
                let validator = match type_str.as_str() {
                    "String" => "'string'".to_string(),
                    "Int" | "Float" => "'number'".to_string(),
                    "Bool" => "CoercedBool".to_string(),
                    "DateTime" => "CoercedDate".to_string(),
                    other => format!("'{}'", other),
                };
                let validator = if col.nullable {
                    format!("{}.or('null')", validator)
                } else {
                    validator
                };
                result.push_str(&format!("  {}: {},\n", col.name, validator));
            }
            _ => {}
        }
    }
    result.push_str("});\n\n");

    result
}

fn generate_db_setup() -> String {
    let mut result = String::new();

    result.push_str("// Database setup for direct SQLite usage\n");
    result.push_str("import { createClient, type Client } from '@libsql/client';\n\n");

    result.push_str("export interface DbConfig {\n");
    result.push_str("  url: string;\n");
    result.push_str("  authToken?: string;\n");
    result.push_str("}\n\n");

    result.push_str("export function createDb(config: DbConfig): Client {\n");
    result.push_str("  return createClient(config);\n");
    result.push_str("}\n");

    result
        .push_str("\nexport function formatResultData(resultSets: any[]): Record<string, any> {\n");
    result.push_str("  const formatted: Record<string, any> = {};\n");
    result.push_str("  for (const resultSet of resultSets) {\n");
    result.push_str("    if (!resultSet?.columns?.length) {\n");
    result.push_str("      continue;\n");
    result.push_str("    }\n");
    result.push_str("    const colName = resultSet.columns[0];\n");
    result.push_str("    if (colName.startsWith('_')) {\n");
    result.push_str("      continue;\n");
    result.push_str("    }\n");
    result.push_str("    for (const row of resultSet.rows || []) {\n");
    result.push_str("      if (colName in row && typeof row[colName] === 'string') {\n");
    result.push_str("        const parsed = JSON.parse(row[colName]);\n");
    result.push_str("        formatted[colName] = Array.isArray(parsed) ? parsed : [parsed];\n");
    result.push_str("        break;\n");
    result.push_str("      }\n");
    result.push_str("    }\n");
    result.push_str("  }\n");
    result.push_str("  return formatted;\n");
    result.push_str("}\n");

    result
}

fn generate_query_function(
    context: &typecheck::Context,
    query_info: &typecheck::QueryInfo,
    query: &ast::Query,
) -> String {
    let mut result = String::new();

    // Imports
    result.push_str("import * as Ark from 'arktype';\n");
    result.push_str("import { type Client } from '@libsql/client';\n");
    result.push_str("import { formatResultData } from '../db';\n");
    result.push_str("import type { Session } from '../types';\n");
    result.push_str("import * as Decode from '../types';\n");

    result.push('\n');

    // Generate Input type
    let input_type_name = format!("{}Input", query.name);
    result.push_str(&generate_input_type(&input_type_name, &query.args));

    // Generate SQL statements (generated at compile time)
    let sql_statements = generate_sql_statements(context, query_info, query);

    // Generate Return type and decoder
    let return_type_name = format!("{}Result", query.name);
    result.push_str(&generate_return_type(context, query, &return_type_name));

    // Generate the async function
    result.push_str(&format!("\nexport async function {}(\n", query.name));
    result.push_str("  db: Client,\n");

    // Add session parameter
    result.push_str("  session: Session");
    if !query.args.is_empty() {
        result.push_str(",\n");
        result.push_str(&format!("  input: {}\n", input_type_name));
    } else {
        result.push_str("\n");
    }
    result.push_str("): Promise<");

    result.push_str(&format!("{}> {{\n", return_type_name));

    // Build args including session args
    result.push_str("  // Build query arguments\n");
    result.push_str("  const args: Record<string, any> = {};\n\n");

    // Add input args
    if !query.args.is_empty() {
        for arg in &query.args {
            result.push_str(&format!("  if (input.{} !== undefined) {{\n", arg.name));
            result.push_str(&format!("    args['{}'] = input.{};\n", arg.name, arg.name));
            result.push_str("  }\n");
        }
    }

    // Add session args
    for (_, param_info) in &query_info.variables {
        match param_info {
            typecheck::ParamInfo::Defined {
                from_session,
                session_name,
                raw_variable_name,
                ..
            } => {
                if *from_session {
                    if let Some(session_name) = session_name {
                        result.push_str(&format!(
                            "  args['{}'] = session.{};\n",
                            raw_variable_name, session_name
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    result.push_str("\n");

    // Execute SQL
    result.push_str("  // Execute SQL statements\n");
    result.push_str("  const results = await db.batch([\n");
    for prepared in &sql_statements {
        result.push_str("    {\n");
        result.push_str(&format!(
            "      sql: `{}`,\n",
            escape_backticks(&prepared.sql)
        ));
        result.push_str("      args,\n");
        result.push_str("    },\n");
    }
    result.push_str("  ]);\n\n");

    result.push_str("  // Decode results\n");
    result.push_str(&format!("  return decode{}Result(results);\n", query.name));

    result.push_str("}\n");

    result
}

fn escape_backticks(s: &str) -> String {
    s.replace("`", "\\`")
}

fn generate_sql_statements(
    context: &typecheck::Context,
    query_info: &typecheck::QueryInfo,
    query: &ast::Query,
) -> Vec<Prepared> {
    let mut statements = Vec::new();

    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                if let Some(table) = context.tables.get(&query_field.name) {
                    let prepared =
                        generate::sql::to_string(context, query, query_info, table, query_field);
                    statements.extend(prepared);
                }
            }
            _ => {}
        }
    }

    statements
}

fn generate_input_type(name: &str, args: &[ast::QueryParamDefinition]) -> String {
    if args.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    result.push_str(&format!("export interface {} {{\n", name));

    for arg in args {
        let ts_type = match arg.type_.as_deref().unwrap_or("unknown") {
            "String" => "string",
            "Int" | "Float" => "number",
            "Bool" => "boolean",
            "DateTime" => "Date",
            other => other,
        };
        result.push_str(&format!("  {}: {};\n", arg.name, ts_type));
    }

    result.push_str("}\n\n");
    result
}

fn generate_return_type(
    context: &typecheck::Context,
    query: &ast::Query,
    return_type_name: &str,
) -> String {
    let mut result = String::new();

    result
        .push_str("const CoercedDate = Ark.type('number').pipe((val) => new Date(val * 1000));\n");
    result.push_str("const CoercedBool = Ark.type('boolean').or('number').pipe((val) => typeof val === 'number' ? val !== 0 : val);\n\n");

    let formatter = to_return_formatter();
    typealias::return_data_aliases(context, query, &mut result, &formatter);

    result.push_str(&format!(
        "export type {} = typeof ReturnData.infer;\n\n",
        return_type_name
    ));

    // Generate decoder function
    result.push_str(&format!(
        "function decode{}Result(results: any[]): {} {{\n",
        query.name, return_type_name
    ));
    result.push_str("  const data = formatResultData(results);\n");
    result.push_str("  const decoded = ReturnData(data);\n");
    result.push_str("  if (decoded instanceof Ark.type.errors) {\n");
    result.push_str("    const errorStr = JSON.stringify(decoded, null, 2);\n");
    result.push_str("    throw new Error(`Failed to decode return data: ${errorStr}`);\n");
    result.push_str("  }\n");
    result.push_str(&format!("  return decoded as {};\n", return_type_name));
    result.push_str("}\n");

    result
}

fn to_return_formatter() -> typealias::TypeFormatter {
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

                let type_str = if needs_coercion {
                    match type_ {
                        "DateTime" => match (is_link, is_array_relationship, is_optional) {
                            (true, true, _) => "CoercedDate.array()".to_string(),
                            (true, false, true) => "CoercedDate.or('null')".to_string(),
                            (true, false, false) => "CoercedDate".to_string(),
                            (false, _, true) => "CoercedDate.or('null')".to_string(),
                            (false, _, false) => "CoercedDate".to_string(),
                        },
                        "Bool" => match (is_link, is_array_relationship, is_optional) {
                            (true, true, _) => "CoercedBool.array()".to_string(),
                            (true, false, true) => "CoercedBool.or('null')".to_string(),
                            (true, false, false) => "CoercedBool".to_string(),
                            (false, _, true) => "CoercedBool.or('null')".to_string(),
                            (false, _, false) => "CoercedBool".to_string(),
                        },
                        _ => unreachable!(),
                    }
                } else {
                    match (is_primitive, is_link, is_array_relationship, is_optional) {
                        (true, true, true, _) => format!("\"{}[]\"", base_type),
                        (true, true, false, true) => format!("\"{} | null\"", base_type),
                        (true, true, false, false) => format!("\"{}\"", base_type),
                        (true, false, _, true) => format!("\"{} | null\"", base_type),
                        (true, false, _, false) => format!("\"{}\"", base_type),
                        (false, true, true, _) => format!("{}.array()", base_type),
                        (false, true, false, true) => format!("{}.or('null')", base_type),
                        (false, true, false, false) => base_type.to_string(),
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

fn generate_queries_index(query_list: &ast::QueryList) -> String {
    let mut result = String::new();

    result.push_str("// Auto-generated exports for all queries\n\n");

    // Export all query functions
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let file_name = string::decapitalize(&q.name);
                result.push_str(&format!(
                    "export {{ {} }} from './queries/{}';\n",
                    q.name, file_name
                ));
            }
            _ => {}
        }
    }

    // Re-export types
    result.push_str("\nexport * from './types';\n");
    result.push_str("export * from './db';\n");

    result
}
