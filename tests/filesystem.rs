use pyre::filesystem;
use std::collections::HashMap;

#[test]
fn test_get_schema_source_matches_suffix_paths() {
    let mut found = filesystem::Found {
        schema_files: HashMap::new(),
        query_files: vec![],
        namespaces: vec![],
    };

    let schema_source = "type DevSource\n   = Browser\n".to_string();
    let schema_file = filesystem::SchemaFile {
        path: "/tmp/project/pyre/schema.pyre".to_string(),
        content: schema_source.clone(),
    };

    found
        .schema_files
        .insert("default".to_string(), vec![schema_file]);

    assert_eq!(
        filesystem::get_schema_source("pyre/schema.pyre", &found),
        Some(schema_source.as_str())
    );
    assert_eq!(
        filesystem::get_schema_source("schema.pyre", &found),
        Some(schema_source.as_str())
    );
}
