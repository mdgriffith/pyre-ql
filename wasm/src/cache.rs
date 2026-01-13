use pyre::db::introspect;
use serde_wasm_bindgen;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

// Use thread_local to store the cached data
thread_local! {
    static CACHED_SCHEMA: Mutex<Option<Arc<introspect::Introspection>>> = Mutex::new(None);
}

pub fn get() -> Option<Arc<introspect::Introspection>> {
    CACHED_SCHEMA.with(|cache| {
        let cache = cache.lock().unwrap();
        match cache.as_ref() {
            Some(introspection) => Some(introspection.clone()),
            None => None,
        }
    })
}

pub fn set_schema(introspection: JsValue) -> () {
    CACHED_SCHEMA.with(|cache| {
        let mut cache = cache.lock().unwrap();

        // Parse and typecheck the schema
        match serde_wasm_bindgen::from_value::<introspect::IntrospectionRaw>(introspection) {
            Ok(introspection_raw) => {
                let introspection = introspect::from_raw(introspection_raw);
                // Cache the result
                *cache = Some(Arc::new(introspection));
            }
            Err(_e) => {
                // Silently fail - error handling can be done at JS level if needed
            }
        }

        ()
    })
}

/// Process introspection raw JSON and return it with links populated
pub fn process_introspection(introspection: JsValue) -> Result<JsValue, JsValue> {
    match serde_wasm_bindgen::from_value::<introspect::IntrospectionRaw>(introspection) {
        Ok(mut introspection_raw) => {
            // Extract links if we have a schema source
            if !introspection_raw.schema_source.is_empty() {
                use pyre::parser;
                use pyre::ast;
                
                let mut schema = ast::Schema {
                    namespace: ast::DEFAULT_SCHEMANAME.to_string(),
                    session: None,
                    files: vec![],
                };
                
                // Parse the schema to extract links
                if parser::run("schema.pyre", &introspection_raw.schema_source, &mut schema).is_ok() {
                    // Extract links from the parsed schema
                    introspection_raw.links = introspect::extract_links(&schema, &introspection_raw.tables);
                }
            }
            // Return the modified introspection_raw with links populated
            serde_wasm_bindgen::to_value(&introspection_raw)
                .map_err(|e| JsValue::from_str(&format!("Failed to serialize: {:?}", e)))
        }
        Err(e) => Err(JsValue::from_str(&format!("Failed to parse introspection: {:?}", e))),
    }
}
