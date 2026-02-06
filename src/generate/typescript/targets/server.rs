use crate::ast;
use crate::filesystem;
use crate::filesystem::generate_text_file;
use crate::typecheck;
use std::collections::HashMap;
use std::path::Path;

pub fn generate_schema(
    _context: &typecheck::Context,
    _database: &ast::Database,
    _base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    let _ = files;
}

pub fn generate_queries(
    _context: &typecheck::Context,
    _all_query_info: &HashMap<String, typecheck::QueryInfo>,
    query_list: &ast::QueryList,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    let mut content = String::new();

    content.push_str("import type { QueryMap, QueryMetadata } from '@pyre/server/query';\n\n");

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
