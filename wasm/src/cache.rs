use pyre::ast;
use pyre::db::introspect;
use pyre::typecheck;
use serde_wasm_bindgen;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

// Use thread_local to store the cached data
thread_local! {
    static CACHED_SCHEMA: Mutex<Option<Arc<introspect::Introspection>>> = Mutex::new(None);
}

pub fn get_or_parse_schema(introspection: JsValue) -> Arc<introspect::Introspection> {
    CACHED_SCHEMA.with(|cache| {
        let mut cache = cache.lock().unwrap();

        // If we have a cached schema, return references to it
        if let Some(introspection_cached) = cache.as_ref() {
            return introspection_cached.clone();
        }

        // Otherwise parse and typecheck the schema
        let introspection_raw: introspect::IntrospectionRaw =
            serde_wasm_bindgen::from_value(introspection).unwrap();

        let introspection = introspect::from_raw(introspection_raw);

        // Cache the result
        *cache = Some(Arc::new(introspection));

        cache.as_ref().unwrap().clone()
    })
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
        let introspection_raw: introspect::IntrospectionRaw =
            serde_wasm_bindgen::from_value(introspection).unwrap();

        let introspection = introspect::from_raw(introspection_raw);

        // Cache the result
        *cache = Some(Arc::new(introspection));

        ()
    })
}
