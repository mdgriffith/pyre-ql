use crate::ast;
use crate::filesystem;
use crate::filesystem::generate_text_file;
use crate::typecheck;
use std::path::Path;

pub fn generate_schema(
    _context: &typecheck::Context,
    _database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    files.push(generate_text_file(
        base_out_dir.join("schema.ts"),
        "export { schemaMetadata } from '../../core/schema';\n".to_string(),
    ));
    files.push(generate_text_file(
        base_out_dir.join("types.ts"),
        "export * from '../../core/types';\n".to_string(),
    ));
}

pub fn generate_queries(
    query_list: &ast::QueryList,
    base_out_dir: &Path,
    files: &mut Vec<filesystem::GeneratedFile<String>>,
) {
    let mut content = String::new();
    content.push_str("// Auto-generated file: collects all query modules for easy importing\n");
    content.push_str("// This file is regenerated when queries are generated\n\n");
    content.push_str("import type { QueryModule } from '@pyre/client/elm-adapter';\n\n");

    let mut query_names: Vec<String> = Vec::new();
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let query_name = q.name.to_string();
                query_names.push(query_name.clone());
                content.push_str(&format!(
                    "import {{ meta as {} }} from '../../core/queries/metadata/{}';\n",
                    query_name,
                    crate::ext::string::decapitalize(&query_name)
                ));
            }
            _ => continue,
        }
    }

    if query_names.is_empty() {
        content.push_str("\nexport const queries: Record<string, QueryModule> = {};\n");
        files.push(generate_text_file(base_out_dir.join("queries.ts"), content));
        return;
    }

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

    files.push(generate_text_file(base_out_dir.join("queries.ts"), content));
}
