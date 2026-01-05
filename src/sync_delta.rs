use crate::ast::{self, WhereArg};
use crate::sync::{SessionValue, TableSyncData};
use crate::typecheck;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Sync delta module requires json feature for JSON value handling
#[cfg(feature = "json")]
use serde_json::Value as JsonValue;

// When json feature is not enabled, sync delta functionality is not available
#[cfg(not(feature = "json"))]
compile_error!("sync_delta module requires the 'json' feature to be enabled");

/// Represents a row that was affected by a mutation
#[derive(Debug, Clone)]
pub struct AffectedRow {
    /// Table name
    pub table_name: String,
    /// Row data as JSON object (column name -> value)
    pub row: JsonValue,
    /// Column names in order
    pub headers: Vec<String>,
}

/// Result of delta generation for a single session
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDelta {
    /// Session identifier
    pub session_id: String,
    /// Delta data organized by table name
    pub tables: HashMap<String, TableSyncData>,
}

/// Generate deltas for all sessions after a mutation
/// Returns a list of deltas, one per session that should receive updates
pub fn generate_deltas(
    affected_rows: &[AffectedRow],
    sessions: &[(String, HashMap<String, SessionValue>)],
    context: &typecheck::Context,
) -> Result<Vec<SessionDelta>, DeltaError> {
    let mut result = Vec::new();

    // Group affected rows by table
    let mut rows_by_table: HashMap<String, Vec<&AffectedRow>> = HashMap::new();
    for row in affected_rows {
        rows_by_table
            .entry(row.table_name.clone())
            .or_insert_with(Vec::new)
            .push(row);
    }

    // For each session, check which tables/rows they can see
    for (session_id, session) in sessions {
        let mut session_tables: HashMap<String, TableSyncData> = HashMap::new();

        // Check each affected table
        for (table_name, rows) in &rows_by_table {
            // Find the table in context
            let table = context
                .tables
                .values()
                .find(|t| {
                    let actual_table_name = ast::get_tablename(&t.record.name, &t.record.fields);
                    actual_table_name == *table_name
                })
                .ok_or_else(|| {
                    DeltaError::TableNotFound(format!("Table {} not found in context", table_name))
                })?;

            // Get permission for select operation
            let permission = ast::get_permissions(&table.record, &ast::QueryOperation::Select);

            // Filter rows that this session can see
            let mut visible_rows: Vec<JsonValue> = Vec::new();
            let mut max_updated_at: Option<i64> = None;

            for row in rows {
                // Check if this session can see this row
                if evaluate_permission(&permission, &row.row, session, table)? {
                    // Convert row to positional array format (matching sync format)
                    let positional_row: Vec<JsonValue> = row
                        .headers
                        .iter()
                        .map(|header| row.row.get(header).cloned().unwrap_or(JsonValue::Null))
                        .collect();

                    visible_rows.push(JsonValue::Array(positional_row));

                    // Track max updatedAt
                    if let Some(JsonValue::Number(n)) = row.row.get("updatedAt") {
                        if let Some(ts) = n.as_i64() {
                            if max_updated_at.is_none() || ts > max_updated_at.unwrap() {
                                max_updated_at = Some(ts);
                            }
                        }
                    }
                }
            }

            // Only add table if there are visible rows
            if !visible_rows.is_empty() {
                // Calculate permission hash
                let permission_hash = crate::sync::calculate_permission_hash(&permission, session);

                session_tables.insert(
                    table_name.clone(),
                    TableSyncData {
                        rows: visible_rows,
                        permission_hash,
                        last_seen_updated_at: max_updated_at,
                    },
                );
            }
        }

        // Only add session delta if there are affected tables
        if !session_tables.is_empty() {
            result.push(SessionDelta {
                session_id: session_id.clone(),
                tables: session_tables,
            });
        }
    }

    Ok(result)
}

/// Evaluate a permission WhereArg against a row of data
/// Returns true if the row matches the permission (session can see it)
fn evaluate_permission(
    permission: &Option<WhereArg>,
    row: &JsonValue,
    session: &HashMap<String, SessionValue>,
    table: &typecheck::Table,
) -> Result<bool, DeltaError> {
    match permission {
        None => Ok(true), // No permission means public access
        Some(perm) => evaluate_where_arg(perm, row, session, table),
    }
}

/// Evaluate a WhereArg against a row
fn evaluate_where_arg(
    where_arg: &WhereArg,
    row: &JsonValue,
    session: &HashMap<String, SessionValue>,
    table: &typecheck::Table,
) -> Result<bool, DeltaError> {
    match where_arg {
        WhereArg::Column(is_session_var, fieldname, op, value) => {
            // Get the left-hand side value
            let lhs_value = if *is_session_var {
                // Field is from session
                session
                    .get(fieldname)
                    .map(|v| session_value_to_json(v))
                    .unwrap_or(JsonValue::Null)
            } else {
                // Field is from table row
                row.get(fieldname).cloned().unwrap_or(JsonValue::Null)
            };

            // Get the right-hand side value
            let rhs_value = evaluate_query_value(value, session)?;

            // Compare using operator
            compare_values(&lhs_value, op, &rhs_value)
        }
        WhereArg::And(args) => {
            // All conditions must be true
            for arg in args {
                if !evaluate_where_arg(arg, row, session, table)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        WhereArg::Or(args) => {
            // At least one condition must be true
            for arg in args {
                if evaluate_where_arg(arg, row, session, table)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

/// Evaluate a QueryValue to a JSON value
fn evaluate_query_value(
    value: &ast::QueryValue,
    session: &HashMap<String, SessionValue>,
) -> Result<JsonValue, DeltaError> {
    match value {
        ast::QueryValue::String((_, s)) => Ok(JsonValue::String(s.clone())),
        ast::QueryValue::Int((_, i)) => Ok(JsonValue::Number((*i as i64).into())),
        ast::QueryValue::Float((_, f)) => {
            // Convert f32 to f64 for JSON
            Ok(JsonValue::Number(
                serde_json::Number::from_f64(*f as f64).unwrap_or(serde_json::Number::from(0)),
            ))
        }
        ast::QueryValue::Bool((_, b)) => Ok(JsonValue::Bool(*b)),
        ast::QueryValue::Null(_) => Ok(JsonValue::Null),
        ast::QueryValue::Variable((_, var)) => {
            // Variable might reference a session field
            if let Some(session_field) = &var.session_field {
                session
                    .get(session_field)
                    .map(|v| session_value_to_json(v))
                    .ok_or_else(|| {
                        DeltaError::EvaluationError(format!(
                            "Session field {} not found",
                            session_field
                        ))
                    })
            } else {
                // Regular variable - not supported in delta evaluation
                Err(DeltaError::EvaluationError(format!(
                    "Variable ${} not supported in delta evaluation",
                    var.name
                )))
            }
        }
        ast::QueryValue::Fn(_) => {
            // Functions not supported in delta evaluation
            Err(DeltaError::EvaluationError(
                "Functions not supported in delta evaluation".to_string(),
            ))
        }
        ast::QueryValue::LiteralTypeValue((_, details)) => {
            // Literal type values - return as string for now
            // In practice, these might need special handling
            Ok(JsonValue::String(details.name.clone()))
        }
    }
}

/// Convert SessionValue to JsonValue
fn session_value_to_json(value: &SessionValue) -> JsonValue {
    match value {
        SessionValue::Null => JsonValue::Null,
        SessionValue::Integer(i) => JsonValue::Number((*i).into()),
        SessionValue::Real(f) => JsonValue::Number(
            serde_json::Number::from_f64(*f).unwrap_or(serde_json::Number::from(0)),
        ),
        SessionValue::Text(s) => JsonValue::String(s.clone()),
        SessionValue::Blob(_) => {
            // Blob not directly supported in JSON, return null
            JsonValue::Null
        }
    }
}

/// Compare two JSON values using an operator
fn compare_values(
    lhs: &JsonValue,
    op: &ast::Operator,
    rhs: &JsonValue,
) -> Result<bool, DeltaError> {
    match op {
        ast::Operator::Equal => Ok(lhs == rhs),
        ast::Operator::NotEqual => Ok(lhs != rhs),
        ast::Operator::GreaterThan => compare_numeric(lhs, rhs, |a, b| a > b),
        ast::Operator::LessThan => compare_numeric(lhs, rhs, |a, b| a < b),
        ast::Operator::GreaterThanOrEqual => compare_numeric(lhs, rhs, |a, b| a >= b),
        ast::Operator::LessThanOrEqual => compare_numeric(lhs, rhs, |a, b| a <= b),
        ast::Operator::In => {
            // rhs should be an array
            if let JsonValue::Array(arr) = rhs {
                Ok(arr.contains(lhs))
            } else {
                Err(DeltaError::EvaluationError(
                    "IN operator requires array on right side".to_string(),
                ))
            }
        }
        ast::Operator::NotIn => {
            // rhs should be an array
            if let JsonValue::Array(arr) = rhs {
                Ok(!arr.contains(lhs))
            } else {
                Err(DeltaError::EvaluationError(
                    "NOT IN operator requires array on right side".to_string(),
                ))
            }
        }
        ast::Operator::Like => {
            // Simple LIKE matching (SQL LIKE with % and _)
            if let (JsonValue::String(lhs_str), JsonValue::String(rhs_pattern)) = (lhs, rhs) {
                Ok(like_match(lhs_str, rhs_pattern))
            } else {
                Ok(false)
            }
        }
        ast::Operator::NotLike => {
            // Simple NOT LIKE matching
            if let (JsonValue::String(lhs_str), JsonValue::String(rhs_pattern)) = (lhs, rhs) {
                Ok(!like_match(lhs_str, rhs_pattern))
            } else {
                Ok(true)
            }
        }
    }
}

/// Compare two JSON values as numbers
fn compare_numeric<F>(lhs: &JsonValue, rhs: &JsonValue, cmp: F) -> Result<bool, DeltaError>
where
    F: FnOnce(f64, f64) -> bool,
{
    let lhs_num = json_to_number(lhs)?;
    let rhs_num = json_to_number(rhs)?;
    Ok(cmp(lhs_num, rhs_num))
}

/// Convert JSON value to number (f64)
fn json_to_number(value: &JsonValue) -> Result<f64, DeltaError> {
    match value {
        JsonValue::Number(n) => n
            .as_f64()
            .ok_or_else(|| DeltaError::EvaluationError("Number too large for f64".to_string())),
        JsonValue::String(s) => s.parse::<f64>().map_err(|_| {
            DeltaError::EvaluationError(format!("Cannot parse string as number: {}", s))
        }),
        _ => Err(DeltaError::EvaluationError(
            "Cannot convert value to number".to_string(),
        )),
    }
}

/// Simple LIKE pattern matching
/// Supports % (any sequence) and _ (single character)
fn like_match(text: &str, pattern: &str) -> bool {
    // Convert SQL LIKE pattern to regex
    let mut regex_pattern = String::new();
    regex_pattern.push('^');
    for ch in pattern.chars() {
        match ch {
            '%' => regex_pattern.push_str(".*"),
            '_' => regex_pattern.push('.'),
            _ => {
                // Escape special regex characters
                if ".+*?^$()[]{}|\\".contains(ch) {
                    regex_pattern.push('\\');
                }
                regex_pattern.push(ch);
            }
        }
    }
    regex_pattern.push('$');

    // Use simple string matching for now (could use regex crate if needed)
    // For simplicity, we'll do a basic implementation
    like_match_simple(text, pattern)
}

/// Simple LIKE matching without regex dependency
fn like_match_simple(text: &str, pattern: &str) -> bool {
    let text_bytes = text.as_bytes();
    let pattern_bytes = pattern.as_bytes();
    like_match_recursive(text_bytes, pattern_bytes)
}

fn like_match_recursive(text: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    match pattern[0] {
        b'%' => {
            // Match zero or more characters
            if pattern.len() == 1 {
                return true; // % at end matches everything
            }
            // Try matching at each position (including empty)
            for i in 0..=text.len() {
                if like_match_recursive(&text[i..], &pattern[1..]) {
                    return true;
                }
            }
            false
        }
        b'_' => {
            // Match exactly one character
            if text.is_empty() {
                return false;
            }
            like_match_recursive(&text[1..], &pattern[1..])
        }
        c => {
            // Match exact character
            if text.is_empty() || text[0] != c {
                return false;
            }
            like_match_recursive(&text[1..], &pattern[1..])
        }
    }
}

#[derive(Debug)]
pub enum DeltaError {
    TableNotFound(String),
    EvaluationError(String),
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaError::TableNotFound(msg) => write!(f, "Table not found: {}", msg),
            DeltaError::EvaluationError(msg) => write!(f, "Evaluation error: {}", msg),
        }
    }
}

impl std::error::Error for DeltaError {}
