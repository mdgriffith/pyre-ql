use crate::ast;
use crate::ast::diff;
use crate::db::introspect;
use crate::error;
use crate::typecheck;

/**
 * This is a dynamic migration approach.
 * It's used in wasm, so it uses no file operations or database stuff.
 *
 *
 */
pub async fn migrate(
    introspection: &introspect::Introspection,
    new_database: &ast::Database,
    namespace: Option<&str>,
) -> Result<String, Vec<error::Error>> {
    // Typecheck the new schema
    let context = typecheck::check_schema(new_database)?;

    // Get the schema based on namespace or single schema
    let new_schema = match namespace {
        Some(ns) => {
            // Look up specific namespace
            new_database
                .schemas
                .iter()
                .find(|s| s.namespace == ns)
                .ok_or_else(|| {
                    vec![error::Error {
                        error_type: error::ErrorType::MigrationSchemaNotFound {
                            namespace: Some(ns.to_string()),
                        },
                        filepath: "".to_string(),
                        locations: vec![],
                    }]
                })?
        }
        None => {
            // Ensure exactly one schema exists
            if new_database.schemas.len() != 1 {
                return Err(vec![error::Error {
                    error_type: error::ErrorType::MigrationSchemaNotFound { namespace: None },
                    filepath: "".to_string(),
                    locations: vec![],
                }]);
            }
            &new_database.schemas[0]
        }
    };

    // Get the recorded schema from introspection
    let db_recorded_schema = introspection.schema.as_ref().ok_or_else(|| {
        vec![error::Error {
            error_type: error::ErrorType::MigrationMissingSchema,
            filepath: "".to_string(),
            locations: vec![],
        }]
    })?;

    // Diff the schemas and check for errors
    let schema_diff = diff::diff_schema(&new_schema, db_recorded_schema);
    let errors = diff::to_errors(schema_diff);
    if !errors.is_empty() {
        return Err(errors);
    }

    // Generate the SQL from the diff
    let db_diff = crate::db::diff::diff(&context, &new_schema, introspection);
    let sql = crate::db::diff::to_sql::to_sql(&db_diff);

    Ok(sql)
}
