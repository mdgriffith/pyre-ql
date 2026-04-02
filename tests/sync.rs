use pyre::ast;
use pyre::parser;
use pyre::sync::{get_sync_sql, SyncCursor, SyncStatusResult, TableSyncStatus};
use pyre::sync_deltas::AffectedRowTableGroup;
use pyre::sync_shape::reshape_table_groups;
use pyre::typecheck;
use serde_json::json;

#[test]
fn sync_sql_marks_json_columns_for_runtime_decoding() {
    let schema_source = r#"
record GameEntity {
    id Int @id
    attrs Json
    updatedAt Int
    @public
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema should parse");

    let database = ast::Database {
        schemas: vec![schema],
    };
    let context = typecheck::check_schema(&database).expect("schema should typecheck");

    let sync_status = SyncStatusResult {
        tables: vec![TableSyncStatus {
            table_name: "gameEntities".to_string(),
            sync_layer: 0,
            needs_sync: true,
            max_updated_at: None,
            permission_hash: "perm".to_string(),
        }],
    };

    let result = match get_sync_sql(
        &sync_status,
        &SyncCursor::new(),
        &context,
        &Default::default(),
        100,
    ) {
        Ok(result) => result,
        Err(_) => panic!("sync sql should generate"),
    };

    assert_eq!(result.tables.len(), 1, "expected one sync table");
    assert_eq!(result.tables[0].json_columns, vec!["attrs".to_string()]);
}

#[test]
fn sync_sql_includes_flattened_custom_type_columns() {
    let schema_source = r#"
type TileFormat
   = Png
   | Webp

type Tiling
   = Tiling {
        tileRootKey String
        tileWidth Int
        format TileFormat
     }

record Map {
    id Int @id
    tiling Tiling?
    updatedAt Int
    @public
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema should parse");

    let database = ast::Database {
        schemas: vec![schema],
    };
    let context = typecheck::check_schema(&database).expect("schema should typecheck");

    let sync_status = SyncStatusResult {
        tables: vec![TableSyncStatus {
            table_name: "maps".to_string(),
            sync_layer: 0,
            needs_sync: true,
            max_updated_at: None,
            permission_hash: "perm".to_string(),
        }],
    };

    let result = match get_sync_sql(
        &sync_status,
        &SyncCursor::new(),
        &context,
        &Default::default(),
        100,
    ) {
        Ok(result) => result,
        Err(_) => panic!("sync sql should generate"),
    };

    assert_eq!(
        result.tables[0].headers,
        vec![
            "id".to_string(),
            "tiling".to_string(),
            "tiling__tileRootKey".to_string(),
            "tiling__tileWidth".to_string(),
            "tiling__format".to_string(),
            "updatedAt".to_string(),
        ]
    );
}

#[test]
fn reshape_table_groups_reconstructs_custom_types_for_sync_payloads() {
    let schema_source = r#"
type TileFormat
   = Png
   | Webp

type Tiling
   = Tiling {
        tileRootKey String
        tileWidth Int
        format TileFormat
     }

record Map {
    id Int @id
    name String
    tiling Tiling?
    updatedAt Int
    @public
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema should parse");

    let database = ast::Database {
        schemas: vec![schema],
    };
    let context = typecheck::check_schema(&database).expect("schema should typecheck");

    let reshaped = reshape_table_groups(
        &[AffectedRowTableGroup {
            table_name: "maps".to_string(),
            headers: vec![
                "id".to_string(),
                "name".to_string(),
                "tiling".to_string(),
                "tiling__tileRootKey".to_string(),
                "tiling__tileWidth".to_string(),
                "tiling__format".to_string(),
                "updatedAt".to_string(),
            ],
            rows: vec![vec![
                json!(1),
                json!("World"),
                json!("Tiling"),
                json!("tiles/root"),
                json!(256),
                json!("Png"),
                json!(1700000000),
            ]],
        }],
        &context,
    );

    assert_eq!(reshaped.len(), 1);
    assert_eq!(
        reshaped[0].headers,
        vec!["id", "name", "tiling", "updatedAt"]
    );
    assert_eq!(
        reshaped[0].rows[0],
        vec![
            json!(1),
            json!("World"),
            json!({
                "type": "Tiling",
                "tileRootKey": "tiles/root",
                "tileWidth": 256,
                "format": { "type": "Png" }
            }),
            json!(1700000000),
        ]
    );
}
