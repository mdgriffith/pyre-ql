use pyre::ast;
use pyre::filesystem::GeneratedFile;
use pyre::generate::typescript::core;
use pyre::parser;
use pyre::typecheck;
use std::path::Path;

#[test]
fn generated_typescript_query_shape_preserves_where_placeholders() {
    let schema_source = r#"
session {
    userId Int
}

record Rulebook {
    @public

    id Id.Int @id
    ownerId Int
    name String
}
"#;

    let query_source = r#"
query GetRulebookByName($name: String) {
    rulebook {
        @where { name == $name && ownerId == Session.userId }

        id
        name
    }
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };
    let context = typecheck::check_schema(&database).expect("schema typechecks");

    let query_list = parser::parse_query("query.pyre", query_source).expect("query parses");
    let query_info = typecheck::check_queries(&query_list, &context).expect("query typechecks");

    let mut files: Vec<GeneratedFile<String>> = Vec::new();
    core::generate_queries(
        &context,
        &query_info,
        &query_list,
        Path::new("typescript/core"),
        &mut files,
    );

    let generated = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .ends_with("queries/metadata/getRulebookByName.ts")
        })
        .expect("generated metadata file");

    let content = &generated.contents;

    assert!(
        content.contains("\"@where\": { \"$and\": [ { \"name\": { \"$var\": \"name\" } }, { \"ownerId\": { \"$session\": \"userId\" } } ] }")
            || content.contains("\"@where\": { \"$and\": [{ \"name\": { \"$var\": \"name\" } }, { \"ownerId\": { \"$session\": \"userId\" } }] }"),
        "TypeScript queryShape should preserve variable and session placeholders in @where. Generated:\n{}",
        content
    );
}
