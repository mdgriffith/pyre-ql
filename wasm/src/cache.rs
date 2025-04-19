use pyre::ast;
use pyre::db::introspect;
use pyre::typecheck;
use serde_wasm_bindgen;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

// Use thread_local to store the cached data
thread_local! {
    static CACHED_SCHEMA: Mutex<Option<(ast::Database, typecheck::Context)>> = Mutex::new(None);
}

pub fn get_or_parse_schema(
    introspection: JsValue,
) -> Result<(ast::Database, typecheck::Context), Vec<pyre::error::Error>> {
    CACHED_SCHEMA.with(|cache| {
        let mut cache = cache.lock().unwrap();

        // If we have a cached schema, return it
        if let Some((db, context)) = cache.as_ref() {
            return Ok((db.clone(), context.clone()));
        }

        // Otherwise parse and typecheck the schema
        let introspection: introspect::Introspection =
            serde_wasm_bindgen::from_value(introspection).unwrap();

        // Create a Database from the introspection
        let mut schema = ast::Schema::default();
        let schema_file = introspect::to_schema::to_schema(&introspection);
        schema.files.push(schema_file);

        let database = ast::Database {
            schemas: vec![schema],
        };

        // Typecheck the schema
        let context = typecheck::check_schema(&database)?;

        // Cache the result
        *cache = Some((database.clone(), context.clone()));

        Ok((database, context))
    })
}
