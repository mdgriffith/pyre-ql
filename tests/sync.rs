use pyre::ast;
use pyre::parser;
use pyre::sync::{get_sync_sql, SyncCursor, SyncStatusResult, TableSyncStatus};
use pyre::typecheck;

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
