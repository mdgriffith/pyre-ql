use crate::ast;
use crate::db::introspect;
use crate::typecheck;

pub struct LoadedSchema {
    introspection: introspect::Introspection,
}

impl LoadedSchema {
    pub fn context(&self) -> Result<&typecheck::Context, Error> {
        context_from_introspection(&self.introspection)
    }

    pub fn schema(&self) -> Result<&ast::Schema, Error> {
        schema_from_introspection(&self.introspection)
    }

    pub fn introspection(&self) -> &introspect::Introspection {
        &self.introspection
    }
}

pub async fn load_schema_from_database(conn: &libsql::Connection) -> Result<LoadedSchema, Error> {
    let is_initialized = is_initialized(conn).await?;
    let sql = if is_initialized {
        introspect::INTROSPECT_SQL
    } else {
        introspect::INTROSPECT_UNINITIALIZED_SQL
    };
    let raw = query_introspection(conn, sql).await?;
    let introspection = introspect::from_raw(raw);
    context_from_introspection(&introspection)?;

    Ok(LoadedSchema { introspection })
}

pub async fn load_context_from_database(conn: &libsql::Connection) -> Result<LoadedSchema, Error> {
    load_schema_from_database(conn).await
}

async fn is_initialized(conn: &libsql::Connection) -> Result<bool, Error> {
    let mut rows = conn
        .query(introspect::IS_INITIALIZED, ())
        .await
        .map_err(Error::Database)?;
    let row = rows.next().await.map_err(Error::Database)?.ok_or({
        Error::InvalidIntrospection("database initialization query returned no rows".to_string())
    })?;
    let value = row.get::<i64>(0).map_err(Error::Database)?;

    Ok(value == 1)
}

async fn query_introspection(
    conn: &libsql::Connection,
    sql: &str,
) -> Result<introspect::IntrospectionRaw, Error> {
    let mut rows = conn.query(sql, ()).await.map_err(Error::Database)?;
    let row = rows.next().await.map_err(Error::Database)?.ok_or({
        Error::InvalidIntrospection("introspection query returned no rows".to_string())
    })?;
    let raw = row.get::<String>(0).map_err(Error::Database)?;

    serde_json::from_str(&raw).map_err(Error::Json)
}

fn context_from_introspection(
    introspection: &introspect::Introspection,
) -> Result<&typecheck::Context, Error> {
    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            if context.tables.is_empty() {
                return Err(Error::MissingSchema);
            }

            Ok(context)
        }
        introspect::SchemaResult::FailedToParse { source, errors } => Err(Error::SchemaParse {
            source: source.clone(),
            errors: errors.clone(),
        }),
        introspect::SchemaResult::FailedToTypecheck { schema: _, errors } => {
            Err(Error::SchemaTypecheck {
                errors: errors.clone(),
            })
        }
    }
}

fn schema_from_introspection(
    introspection: &introspect::Introspection,
) -> Result<&ast::Schema, Error> {
    match &introspection.schema {
        introspect::SchemaResult::Success { schema, .. } => Ok(schema),
        introspect::SchemaResult::FailedToParse { source, errors } => Err(Error::SchemaParse {
            source: source.clone(),
            errors: errors.clone(),
        }),
        introspect::SchemaResult::FailedToTypecheck { schema: _, errors } => {
            Err(Error::SchemaTypecheck {
                errors: errors.clone(),
            })
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Database(libsql::Error),
    InvalidIntrospection(String),
    Json(serde_json::Error),
    MissingSchema,
    SchemaParse {
        source: String,
        errors: Vec<crate::error::Error>,
    },
    SchemaTypecheck {
        errors: Vec<crate::error::Error>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Database(error) => write!(f, "database error: {}", error),
            Error::InvalidIntrospection(message) => write!(f, "invalid introspection: {}", message),
            Error::Json(error) => write!(f, "json error: {}", error),
            Error::MissingSchema => write!(f, "database does not contain a Pyre schema"),
            Error::SchemaParse { errors, .. } => {
                write!(f, "schema failed to parse with {} error(s)", errors.len())
            }
            Error::SchemaTypecheck { errors } => {
                write!(
                    f,
                    "schema failed to typecheck with {} error(s)",
                    errors.len()
                )
            }
        }
    }
}

impl std::error::Error for Error {}
