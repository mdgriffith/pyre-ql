use crate::sync::{self, SyncCursor, SyncPageResult, TableSyncData};
use crate::sync_deltas::{self, AffectedRowTableGroup};
use crate::sync_shape;
use crate::typecheck;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

pub type SyncSession = HashMap<String, sync::SessionValue>;
pub type ConnectedSessions = HashMap<String, SyncSession>;

pub struct SyncServer<'a> {
    context: &'a typecheck::Context,
}

impl<'a> SyncServer<'a> {
    pub fn new(context: &'a typecheck::Context) -> Self {
        Self { context }
    }

    pub async fn catchup(
        &self,
        conn: &libsql::Connection,
        sync_cursor: &SyncCursor,
        session: &SyncSession,
        page_size: usize,
    ) -> Result<SyncPageResult, Error> {
        catchup(conn, self.context, sync_cursor, session, page_size).await
    }

    pub fn calculate_deltas(
        &self,
        affected_row_groups: &[AffectedRowTableGroup],
        connected_sessions: &ConnectedSessions,
    ) -> Result<Vec<SessionDeltaMessage>, Error> {
        calculate_deltas(self.context, affected_row_groups, connected_sessions)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeltaMessage {
    #[serde(rename = "type")]
    pub type_: String,
    pub data: Vec<AffectedRowTableGroup>,
}

impl DeltaMessage {
    pub fn delta(data: Vec<AffectedRowTableGroup>) -> Self {
        Self {
            type_: "delta".to_string(),
            data,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionDeltaMessage {
    pub session_id: String,
    pub message: DeltaMessage,
}

/// Run a catchup sync request using a client cursor and logical session values.
pub async fn catchup(
    conn: &libsql::Connection,
    context: &typecheck::Context,
    sync_cursor: &SyncCursor,
    session: &SyncSession,
    page_size: usize,
) -> Result<SyncPageResult, Error> {
    if page_size == 0 {
        return Err(Error::InvalidPageSize);
    }

    let status_sql =
        sync::get_sync_status_sql(sync_cursor, context, session).map_err(Error::Sync)?;
    let status_rows = query_objects(conn, &status_sql).await?;
    let sync_status = sync::parse_sync_status(sync_cursor, context, session, &status_rows)
        .map_err(Error::Sync)?;
    let sync_sql = sync::get_sync_sql(&sync_status, sync_cursor, context, session, page_size)
        .map_err(Error::Sync)?;

    let mut result = SyncPageResult {
        tables: HashMap::new(),
        has_more: false,
    };

    for table_sql in sync_sql.tables {
        let updated_at_index = table_sql
            .headers
            .iter()
            .position(|header| header == "updatedAt");
        let mut table_rows = Vec::new();
        let mut max_updated_at = None;

        for statement in &table_sql.sql {
            let rows = query_objects(conn, statement).await?;

            for mut row in rows {
                decode_json_columns(&mut row, &table_sql.json_columns)?;

                if updated_at_index.is_some() {
                    if let Some(updated_at) = row.get("updatedAt").and_then(json_to_i64) {
                        max_updated_at = Some(match max_updated_at {
                            Some(current) if current >= updated_at => current,
                            _ => updated_at,
                        });
                    }
                }

                table_rows.push(row);
            }
        }

        let has_more_for_table = table_rows.len() > page_size;
        if has_more_for_table {
            table_rows.truncate(page_size);
            result.has_more = true;

            if updated_at_index.is_some() {
                max_updated_at = table_rows
                    .last()
                    .and_then(|row| row.get("updatedAt"))
                    .and_then(json_to_i64);
            }
        }

        let raw_group = AffectedRowTableGroup {
            table_name: table_sql.table_name.clone(),
            headers: table_sql.headers.clone(),
            rows: table_rows
                .iter()
                .map(|row| {
                    table_sql
                        .headers
                        .iter()
                        .map(|header| row.get(header).cloned().unwrap_or(JsonValue::Null))
                        .collect()
                })
                .collect(),
        };

        let reshaped_group = sync_shape::reshape_table_groups(&[raw_group], context)
            .into_iter()
            .next();
        let rows = reshaped_group
            .map(|group| {
                group
                    .rows
                    .into_iter()
                    .map(|row| row_array_to_object(&group.headers, row))
                    .collect()
            })
            .unwrap_or_default();

        result.tables.insert(
            table_sql.table_name,
            TableSyncData {
                rows,
                permission_hash: table_sql.permission_hash,
                last_seen_updated_at: max_updated_at,
            },
        );
    }

    Ok(result)
}

/// Calculate permission-filtered live delta messages from mutation affected rows.
pub fn calculate_deltas(
    context: &typecheck::Context,
    affected_row_groups: &[AffectedRowTableGroup],
    connected_sessions: &ConnectedSessions,
) -> Result<Vec<SessionDeltaMessage>, Error> {
    if affected_row_groups.is_empty() || connected_sessions.is_empty() {
        return Ok(Vec::new());
    }

    let result =
        sync_deltas::calculate_sync_deltas(affected_row_groups, connected_sessions, context)
            .map_err(Error::SyncDeltas)?;
    let mut messages = Vec::new();

    for group in result.groups {
        let reshaped_table_groups = sync_shape::reshape_table_groups(&group.table_groups, context);
        let message = DeltaMessage::delta(reshaped_table_groups);

        for session_id in group.session_ids {
            messages.push(SessionDeltaMessage {
                session_id,
                message: message.clone(),
            });
        }
    }

    messages.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok(messages)
}

async fn query_objects(
    conn: &libsql::Connection,
    sql: &str,
) -> Result<Vec<HashMap<String, JsonValue>>, Error> {
    let mut rows = conn.query(sql, ()).await.map_err(Error::Database)?;
    let column_names = (0..rows.column_count())
        .map(|index| rows.column_name(index).unwrap_or("").to_string())
        .collect::<Vec<_>>();
    let mut result = Vec::new();

    while let Some(row) = rows.next().await.map_err(Error::Database)? {
        let mut object = HashMap::with_capacity(column_names.len());

        for (index, column_name) in column_names.iter().enumerate() {
            let value = row
                .get::<libsql::Value>(index as i32)
                .map_err(Error::Database)?;
            object.insert(column_name.clone(), libsql_value_to_json(value));
        }

        result.push(object);
    }

    Ok(result)
}

fn libsql_value_to_json(value: libsql::Value) -> JsonValue {
    match value {
        libsql::Value::Null => JsonValue::Null,
        libsql::Value::Integer(value) => JsonValue::from(value),
        libsql::Value::Real(value) => JsonValue::from(value),
        libsql::Value::Text(value) => JsonValue::String(value),
        libsql::Value::Blob(value) => {
            JsonValue::Array(value.into_iter().map(JsonValue::from).collect())
        }
    }
}

fn decode_json_columns(
    row: &mut HashMap<String, JsonValue>,
    json_columns: &[String],
) -> Result<(), Error> {
    for column in json_columns {
        if let Some(value) = row.get(column).cloned() {
            row.insert(column.clone(), parse_json_column_value(value)?);
        }
    }

    Ok(())
}

fn parse_json_column_value(value: JsonValue) -> Result<JsonValue, Error> {
    match value {
        JsonValue::String(raw) => {
            let parsed = serde_json::from_str::<JsonValue>(&raw).map_err(Error::Json)?;
            Ok(try_parse_nested_json_container(parsed))
        }
        value => Ok(value),
    }
}

fn try_parse_nested_json_container(value: JsonValue) -> JsonValue {
    let JsonValue::String(raw) = value else {
        return value;
    };

    let trimmed = raw.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return JsonValue::String(raw);
    }

    serde_json::from_str::<JsonValue>(trimmed).unwrap_or(JsonValue::String(raw))
}

fn row_array_to_object(headers: &[String], row: Vec<JsonValue>) -> JsonValue {
    let mut object = serde_json::Map::with_capacity(headers.len());

    for (index, header) in headers.iter().enumerate() {
        object.insert(
            header.clone(),
            row.get(index).cloned().unwrap_or(JsonValue::Null),
        );
    }

    JsonValue::Object(object)
}

fn json_to_i64(value: &JsonValue) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|value| value as i64))
}

#[derive(Debug)]
pub enum Error {
    Database(libsql::Error),
    InvalidPageSize,
    Json(serde_json::Error),
    Sync(sync::SyncError),
    SyncDeltas(sync_deltas::SyncDeltasError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Database(error) => write!(f, "database error: {}", error),
            Error::InvalidPageSize => write!(f, "page_size must be greater than zero"),
            Error::Json(error) => write!(f, "json error: {}", error),
            Error::Sync(sync::SyncError::DatabaseError(message)) => {
                write!(f, "sync database error: {}", message)
            }
            Error::Sync(sync::SyncError::SqlGenerationError(message)) => {
                write!(f, "sync sql generation error: {}", message)
            }
            Error::Sync(sync::SyncError::PermissionError(message)) => {
                write!(f, "sync permission error: {}", message)
            }
            Error::SyncDeltas(sync_deltas::SyncDeltasError::TableNotFound(table_name)) => {
                write!(f, "sync delta table not found: {}", table_name)
            }
            Error::SyncDeltas(sync_deltas::SyncDeltasError::InvalidRowData(message)) => {
                write!(f, "sync delta invalid row data: {}", message)
            }
        }
    }
}

impl std::error::Error for Error {}
