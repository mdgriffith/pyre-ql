use pyre::ast;
use pyre::filesystem::GeneratedFile;
use pyre::generate::server::typescript;
use pyre::generate::typescript::core;
use pyre::parser;
use pyre::typecheck;
use std::path::Path;

#[test]
fn typescript_schema_and_decoders_render_typed_json_containers() {
    let schema_source = r#"
type Lifecycle
   = Running
   | Finished {
        reason String
     }

record Event {
    @public
    id       Id.Int @id
    payload  Json<Lifecycle>
    tags     Json<List<String>>
    counts   Json<Dict<Int>>
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let schema_ts = typescript::schema(&database);

    let query_source = r#"
insert CreateEvent($payload: Json<Lifecycle>, $tags: Json<List<String>>, $counts: Json<Dict<Int>>) {
    event {
        payload = $payload
        tags = $tags
        counts = $counts
    }
}
"#;

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

    let metadata = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .ends_with("queries/metadata/createEvent.ts")
        })
        .expect("generated metadata file");

    let content = &metadata.contents;

    assert!(
        schema_ts.contains("\"payload\": Lifecycle;")
            && schema_ts.contains("\"tags\": Array<string>;")
            && schema_ts.contains("\"counts\": Record<string, number>;")
            && schema_ts.contains("\"type_\": \"Finished\";"),
        "Expected typed Json fields to surface as rich TypeScript types. Generated schema:\n{}",
        schema_ts
    );

    assert!(
        content.contains("payload: Decode.Lifecycle")
            && content.contains("tags: z.array(z.string())")
            && content.contains("counts: z.record(z.number())")
            && content.contains("json_input_args: [\"payload\", \"tags\", \"counts\"]"),
        "Expected typed Json fields to use recursive TypeScript query validators. Generated metadata:\n{}",
        content
    );
}
