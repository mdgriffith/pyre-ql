#[allow(dead_code, unused_imports)]
mod helpers;

use helpers::test_database::TestDatabase;
use pyre::server::manifest::{Manifest, PyreSession, QueryManifest};
use pyre::server::query;
use serde_json::json;

fn manifest_for(
    context: &pyre::typecheck::Context,
    query_source: &str,
    include_generated_crud: bool,
) -> Result<Manifest, Box<dyn std::error::Error>> {
    let mut query_list = if query_source.trim().is_empty() {
        pyre::ast::QueryList {
            queries: Vec::new(),
        }
    } else {
        pyre::parser::parse_query("query.pyre", query_source)
            .map_err(|err| format!("query parse failed: {:?}", err))?
    };

    if include_generated_crud {
        pyre::generated_queries::append_generated_crud_queries(&mut query_list, context);
    }

    let query_info = pyre::typecheck::check_queries(&query_list, context)
        .map_err(|errors| format!("query typecheck failed: {:?}", errors))?;
    let mut files = Vec::new();
    pyre::generate::manifest::generate_queries(context, &query_list, &query_info, &mut files);
    let manifest_file = files
        .into_iter()
        .find(|file| file.path == std::path::Path::new("manifest.json"))
        .ok_or("manifest file should be generated")?;

    Ok(serde_json::from_str(&manifest_file.contents)?)
}

fn only_query(manifest: &Manifest) -> &QueryManifest {
    manifest
        .queries
        .values()
        .next()
        .expect("manifest should contain a query")
}

fn query_by_operation<'a>(manifest: &'a Manifest, operation: &str) -> &'a QueryManifest {
    manifest
        .queries
        .values()
        .find(|query| query.operation == operation)
        .expect("query should exist for operation")
}

#[tokio::test]
async fn run_select_query_formats_response() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDatabase::new(
        r#"
record Note {
    id Int @id
    body String
    updatedAt Int
    @public
}
"#,
    )
    .await?;
    let conn = db.db.connect()?;
    conn.execute_batch("insert into notes (id, body, updatedAt) values (1, 'one', 10);")
        .await?;
    let manifest = manifest_for(
        &db.context,
        r#"
query GetNotes {
    note {
        id
        body
    }
}
"#,
        false,
    )?;
    let session = PyreSession::new(json!({}), &manifest.session_schema)?;
    let result = query::run(
        &conn,
        &manifest,
        &only_query(&manifest).id,
        json!({}),
        &session,
    )
    .await?;

    assert_eq!(result.response["note"][0]["id"], json!(1));
    assert_eq!(result.response["note"][0]["body"], json!("one"));
    assert!(result.affected_rows.is_empty());

    Ok(())
}

#[tokio::test]
async fn run_insert_mutation_extracts_affected_rows() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDatabase::new(
        r#"
record Note {
    id Int @id
    body String
    updatedAt Int
    @public
}
"#,
    )
    .await?;
    let conn = db.db.connect()?;
    let manifest = manifest_for(
        &db.context,
        r#"
insert CreateNote($body: String) {
    note {
        body = $body
        updatedAt = 10
        id
    }
}
"#,
        false,
    )?;
    let session = PyreSession::new(json!({}), &manifest.session_schema)?;
    let result = query::run(
        &conn,
        &manifest,
        &only_query(&manifest).id,
        json!({ "body": "one" }),
        &session,
    )
    .await?;

    assert_eq!(result.response["note"][0]["body"], json!("one"));
    assert_eq!(result.affected_rows.len(), 1);
    assert_eq!(result.affected_rows[0].table_name, "notes");
    assert_eq!(result.affected_rows[0].rows.len(), 1);

    Ok(())
}

#[tokio::test]
async fn run_query_applies_json_and_session_args() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDatabase::new(
        r#"
session {
    userId Int
}

record Note {
    id Int @id
    ownerId Int
    attrs Json
    updatedAt Int
    @allow(*) { ownerId == Session.userId }
}
"#,
    )
    .await?;
    let conn = db.db.connect()?;
    let manifest = manifest_for(
        &db.context,
        r#"
insert CreateNote($attrs: Json) {
    note {
        ownerId = Session.userId
        attrs = $attrs
        updatedAt = 10
        id
    }
}
"#,
        false,
    )?;
    let session = PyreSession::new(json!({ "userId": 7 }), &manifest.session_schema)?;
    let result = query::run(
        &conn,
        &manifest,
        &only_query(&manifest).id,
        json!({ "attrs": { "theme": "forest" } }),
        &session,
    )
    .await?;

    assert_eq!(result.affected_rows[0].rows.len(), 1);
    let mut rows = conn
        .query("select json(attrs) from notes where ownerId = 7", ())
        .await?;
    let row = rows.next().await?.expect("inserted row should exist");
    let attrs = serde_json::from_str::<serde_json::Value>(&row.get::<String>(0)?)?;
    assert_eq!(attrs, json!({ "theme": "forest" }));

    Ok(())
}

#[tokio::test]
async fn generated_update_respects_omitted_vs_null_optional_args(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDatabase::new(
        r#"
record Note {
    id Int @id
    body String?
    updatedAt Int
    @public
}
"#,
    )
    .await?;
    let conn = db.db.connect()?;
    conn.execute_batch("insert into notes (id, body, updatedAt) values (1, 'old', 10);")
        .await?;
    let manifest = manifest_for(
        &db.context,
        r#"
query GetNotes {
    note {
        id
        body
    }
}
"#,
        true,
    )?;
    let update = query_by_operation(&manifest, "update");
    let session = PyreSession::new(json!({}), &manifest.session_schema)?;

    query::run(&conn, &manifest, &update.id, json!({ "id": 1 }), &session).await?;
    let after_omitted = query::run(
        &conn,
        &manifest,
        &query_by_operation(&manifest, "query").id,
        json!({}),
        &session,
    )
    .await?;
    assert_eq!(after_omitted.response["note"][0]["body"], json!("old"));

    query::run(
        &conn,
        &manifest,
        &update.id,
        json!({ "id": 1, "body": null }),
        &session,
    )
    .await?;
    let after_null = query::run(
        &conn,
        &manifest,
        &query_by_operation(&manifest, "query").id,
        json!({}),
        &session,
    )
    .await?;
    assert_eq!(after_null.response["note"][0]["body"], json!(null));

    Ok(())
}

#[tokio::test]
async fn run_query_reports_unknown_and_invalid_input() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDatabase::new(
        r#"
record Note {
    id Int @id
    body String
    updatedAt Int
    @public
}
"#,
    )
    .await?;
    let conn = db.db.connect()?;
    let manifest = manifest_for(
        &db.context,
        r#"
query GetNote($id: Int) {
    note {
        @where { id == $id }
        id
    }
}
"#,
        false,
    )?;
    let session = PyreSession::new(json!({}), &manifest.session_schema)?;

    let unknown = query::run(&conn, &manifest, "missing", json!({}), &session)
        .await
        .expect_err("unknown query should fail");
    assert_eq!(unknown.to_string(), "unknown query: missing");

    let invalid = query::run(
        &conn,
        &manifest,
        &only_query(&manifest).id,
        json!({ "id": "nope" }),
        &session,
    )
    .await
    .expect_err("invalid input should fail");
    assert_eq!(
        invalid.to_string(),
        "invalid input: input field 'id' must be Int"
    );

    Ok(())
}
