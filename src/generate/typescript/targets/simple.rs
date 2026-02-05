use crate::ast;
use crate::ext::string;
use crate::filesystem;
use crate::filesystem::generate_text_file;
use crate::typecheck;
use std::collections::HashMap;
use std::path::Path;

pub fn generate_schema(
    _database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    files.push(generate_text_file(
        base_out_dir.join("db.ts"),
        generate_db_setup(),
    ));
}

pub fn generate_queries(
    context: &typecheck::Context,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    query_list: &ast::QueryList,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
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

    files.push(generate_text_file(
        base_out_dir.join("index.ts"),
        generate_queries_index(query_list),
    ));
}

fn generate_db_setup() -> String {
    let mut result = String::new();

    result.push_str("// Database setup for direct SQLite usage\n");
    result.push_str("import { createClient, type Client } from '@libsql/client';\n");
    result.push_str("export { buildArgs, formatResultData, toSqlStatements } from '../../core/runtime/sql';\n\n");

    result.push_str("export interface DbConfig {\n");
    result.push_str("  url: string;\n");
    result.push_str("  authToken?: string;\n");
    result.push_str("}\n\n");

    result.push_str("export function createDb(config: DbConfig): Client {\n");
    result.push_str("  return createClient(config);\n");
    result.push_str("}\n");

    result
}

fn generate_query_function(
    _context: &typecheck::Context,
    _query_info: &typecheck::QueryInfo,
    query: &ast::Query,
) -> String {
    let mut result = String::new();

    let query_file = string::decapitalize(&query.name);

    result.push_str("import type { Client } from '@libsql/client';\n");
    result.push_str("import { buildArgs, formatResultData, toSqlStatements } from '../db';\n");
    result.push_str("import type { Session } from '../../../core/types';\n");
    result.push_str("import { decodeOrThrow } from '../../../core/codec';\n");
    result.push_str(&format!(
        "import type {{ Input, Result }} from '../../../core/queries/metadata/{}';\n",
        query_file
    ));
    result.push_str(&format!(
        "import {{ meta as {}Meta }} from '../../../core/queries/metadata/{}';\n",
        query.name, query_file
    ));
    result.push_str(&format!(
        "import {{ sql }} from '../../../core/queries/sql/{}';\n\n",
        query_file
    ));

    let input_type_name = format!("{}Input", query.name);
    let return_type_name = format!("{}Result", query.name);

    if !query.args.is_empty() {
        result.push_str(&format!("export type {} = Input;\n", input_type_name));
    }
    result.push_str(&format!("export type {} = Result;\n\n", return_type_name));

    result.push_str(&format!("export async function {}(\n", query.name));
    result.push_str("  db: Client,\n");
    result.push_str("  session: Session");
    if !query.args.is_empty() {
        result.push_str(",\n");
        result.push_str(&format!("  input: {}\n", input_type_name));
    } else {
        result.push_str("\n");
    }
    result.push_str(&format!("): Promise<{}> {{\n", return_type_name));

    result.push_str("  const args = buildArgs(");
    if query.args.is_empty() {
        result.push_str("undefined");
    } else {
        result.push_str("input");
    }
    result.push_str(&format!(", session, {}Meta.session_args);\n\n", query.name));

    result.push_str("  const results = await db.batch(toSqlStatements(sql, args));\n\n");

    result.push_str("  const data = formatResultData(sql, results);\n");
    result.push_str(&format!(
        "  return decodeOrThrow({}Meta.ReturnData, data, 'return data');\n",
        query.name
    ));
    result.push_str("}\n");

    result
}

fn generate_queries_index(query_list: &ast::QueryList) -> String {
    let mut result = String::new();

    result.push_str("// Auto-generated exports for all queries\n\n");

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

    result.push_str("\nexport * from '../../core/types';\n");
    result.push_str("export * from './db';\n");

    result
}
