use crate::ast;
use crate::ext::string;
use crate::generate::sql;
use crate::typecheck;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn operation_name(operation: &ast::QueryOperation) -> String {
    match operation {
        ast::QueryOperation::Select => "Queried",
        ast::QueryOperation::Insert => "Added",
        ast::QueryOperation::Update => "Updated",
        ast::QueryOperation::Delete => "Deleted",
    }
    .to_string()
}

pub fn write(dir: &Path, context: &typecheck::Context) {
    let target_path = dir.join("watched.ts");

    let mut content = String::new();

    content
        .push_str("\n\n// All tables that are currently being watched\nexport enum WatchedKind {");
    let mut at_least_one_watched = false;
    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            content.push_str(&format!(
                "\n  {}{} = {},",
                record.name,
                operation_name(&watched_operation),
                string::quote(&format!(
                    "{}{}",
                    record.name,
                    operation_name(&watched_operation)
                ))
            ));
        }
    }
    content.push_str("\n}");

    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            let name = format!("{}{}", record.name, operation_name(&watched_operation));
            content.push_str(&format!(
                "\n\nexport interface {} {{\n  kind: WatchedKind.{};\n  data: {};\n}}",
                name, name, "{}"
            ));
        }
    }

    content.push_str("\n\nexport type Watched");
    let mut at_least_one_constructor = false;
    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            let name = format!("{}{}", record.name, operation_name(&watched_operation));
            if !at_least_one_constructor {
                content.push_str(&format!("\n    = {}", name));
                at_least_one_constructor = true;
            } else {
                content.push_str(&format!("\n    | {}", name));
            }
        }
    }
    if !at_least_one_constructor {
        content.push_str(" = {};")
    } else {
        content.push_str("\n\n");
        write_runner(context, &mut content);
    }

    let mut output = fs::File::create(target_path).expect("Failed to create file");
    output
        .write_all(content.as_bytes())
        .expect("Failed to write to file");
}

fn write_runner(context: &typecheck::Context, content: &mut String) {
    content.push_str("export interface Operations {\n");

    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            content.push_str(&format!(
                "  {}{}: (env: any) => void;\n",
                string::decapitalize(&record.name),
                operation_name(&watched_operation)
            ));
        }
    }

    content.push_str("}\n\n\n");

    // Executor
    content.push_str("export default function exec(\n");
    content.push_str("  env: any,\n");
    content.push_str("  operations: Operations,\n");
    content.push_str("  watched: Watched[],\n");
    content.push_str(") {\n");
    content.push_str("  watched.forEach((event) => {\n");
    content.push_str("    switch (event.kind) {\n");

    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            content.push_str(&format!(
                "\n      case WatchedKind.{}{}:\n        {}",
                record.name,
                operation_name(&watched_operation),
                &format!(
                    "operations.{}{}(env);\n",
                    string::decapitalize(&record.name),
                    operation_name(&watched_operation)
                )
            ));

            content.push_str("        break;\n")
        }
    }

    content.push_str("    }\n");
    content.push_str("  });\n");
    content.push_str("}\n");
}
