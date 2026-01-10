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
