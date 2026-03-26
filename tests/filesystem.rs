use pyre::filesystem;
use std::collections::HashMap;
use tempfile::TempDir;

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

#[test]
fn test_get_namespace_for_nested_schema_file() {
    let base_dir = std::path::Path::new("/tmp/project/pyre");
    let nested_path = std::path::Path::new("/tmp/project/pyre/schema/Auth/schema.pyre");

    let namespace = filesystem::get_namespace(nested_path, base_dir);
    assert_eq!(namespace, "Auth");
}

#[test]
fn test_collect_filepaths_groups_schema_files_by_namespace() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let root = temp_dir.path();

    std::fs::create_dir_all(root.join("pyre/schema/App")).expect("App schema dir should exist");
    std::fs::create_dir_all(root.join("pyre/schema/Auth")).expect("Auth schema dir should exist");

    std::fs::write(
        root.join("pyre/schema/App/schema.pyre"),
        "record Project {\n    id Int @id\n    @public\n}\n",
    )
    .expect("App schema file should be written");

    std::fs::write(
        root.join("pyre/schema/Auth/schema.pyre"),
        "record Account {\n    id Int @id\n    @public\n}\n",
    )
    .expect("Auth schema file should be written");

    let found = filesystem::collect_filepaths(root.join("pyre").as_path())
        .expect("collect_filepaths should succeed");

    assert!(found.schema_files.contains_key("App"));
    assert!(found.schema_files.contains_key("Auth"));
    assert_eq!(
        found.schema_files.get("App").map(|files| files.len()),
        Some(1)
    );
    assert_eq!(
        found.schema_files.get("Auth").map(|files| files.len()),
        Some(1)
    );
}
