use crate::ast;
use crate::filesystem;
use crate::filesystem::generate_text_file;
use crate::generate;
use crate::generate::server::typescript::watched;
use crate::typecheck;
use std::collections::HashMap;
use std::path::Path;

const DB_ENGINE: &str = include_str!("../../server/static/typescript/db.ts");

pub fn generate_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    files.push(generate_text_file(
        base_out_dir.join("db.ts"),
        DB_ENGINE.to_string(),
    ));

    if let Some(config_ts) = to_env(database) {
        files.push(generate_text_file(
            base_out_dir.join("db/env.ts"),
            config_ts,
        ));
    }

    files.push(generate_text_file(
        base_out_dir.join("db/data.ts"),
        generate::server::typescript::schema(database),
    ));
    files.push(generate_text_file(
        base_out_dir.join("db/decode.ts"),
        generate::server::typescript::to_schema_decoders(database),
    ));

    watched::generate(files, context, base_out_dir);
}

pub fn generate_queries(
    _context: &typecheck::Context,
    _all_query_info: &HashMap<String, typecheck::QueryInfo>,
    query_list: &ast::QueryList,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    let mut content = String::new();

    content.push_str(
        "import type { QueryMap, QueryMetadata } from '../../../../../../wasm/server';\n",
    );

    for operation in &query_list.queries {
        if let ast::QueryDef::Query(q) = operation {
            let query_name = q.name.to_string();
            content.push_str(&format!(
                "import {{ meta as {} }} from '../../core/queries/metadata/{}';\n",
                query_name,
                crate::ext::string::decapitalize(&query_name)
            ));
            content.push_str(&format!(
                "import {{ sql as {}Sql }} from '../../core/queries/sql/{}';\n",
                query_name,
                crate::ext::string::decapitalize(&query_name)
            ));
        }
    }

    content.push_str("\n");

    for operation in &query_list.queries {
        if let ast::QueryDef::Query(q) = operation {
            content.push_str(&format!(
                "const {}Query: QueryMetadata = {{\n  ...{},\n  sql: {}Sql\n}};\n\n",
                q.name, q.name, q.name
            ));
        }
    }

    content.push_str("export const queries: QueryMap = {\n");
    let mut first = true;
    for operation in &query_list.queries {
        if let ast::QueryDef::Query(q) = operation {
            if !first {
                content.push_str(",\n");
            }
            content.push_str(&format!("  [{}.id]: {}Query", q.name, q.name));
            first = false;
        }
    }
    content.push_str("\n};\n");

    files.push(generate_text_file(base_out_dir.join("queries.ts"), content));
}

fn to_env(database: &ast::Database) -> Option<String> {
    generate::server::typescript::to_env(database)
}
