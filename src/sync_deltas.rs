use crate::ast::{self, WhereArg};
use crate::sync::SessionValue;
use crate::typecheck;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Sync deltas module requires json feature for JSON value handling
#[cfg(feature = "json")]
use serde_json::Value as JsonValue;

// When json feature is not enabled, sync deltas functionality is not available
#[cfg(not(feature = "json"))]
compile_error!("sync_deltas module requires the 'json' feature to be enabled");

/// A single affected row from a mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedRow {
    pub table_name: String,
    pub row: JsonValue,       // Object with column names as keys
    pub headers: Vec<String>, // Column names in order
}

/// A connected session with its identifier and values
#[derive(Debug, Clone)]
pub struct ConnectedSession {
    pub session_id: String,
    pub session: HashMap<String, SessionValue>,
}

/// Delta for a single session - list of affected rows that session should receive
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDelta {
    pub session_id: String,
    pub affected_rows: Vec<AffectedRow>,
}

/// Result containing deltas for all sessions that should receive updates
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncDeltasResult {
    pub deltas: Vec<SessionDelta>,
}

/// Evaluate a permission WhereArg against row data and session values
/// Returns true if the permission condition is satisfied
fn evaluate_permission(
    where_arg: &WhereArg,
    row_data: &HashMap<String, JsonValue>,
    session: &HashMap<String, SessionValue>,
) -> bool {
    match where_arg {
        WhereArg::Column(is_session_var, fieldname, op, value) => {
            // Get the left-hand side value
            let lhs_value = if *is_session_var {
                // Session variable - get from session
                session
                    .get(fieldname)
                    .map(|v| session_value_to_json(v))
                    .unwrap_or(JsonValue::Null)
            } else {
                // Table column - get from row data
                row_data.get(fieldname).cloned().unwrap_or(JsonValue::Null)
            };

            // Get the right-hand side value
            let rhs_value = query_value_to_json(value, session);

            // Evaluate the operator
            evaluate_operator(op, &lhs_value, &rhs_value)
        }
        WhereArg::And(args) => {
            // All conditions must be true
            args.iter()
                .all(|arg| evaluate_permission(arg, row_data, session))
        }
        WhereArg::Or(args) => {
            // At least one condition must be true
            args.iter()
                .any(|arg| evaluate_permission(arg, row_data, session))
        }
    }
}

/// Evaluate an operator between two JSON values
fn evaluate_operator(op: &ast::Operator, lhs: &JsonValue, rhs: &JsonValue) -> bool {
    match op {
        ast::Operator::Equal => json_values_equal(lhs, rhs),
        ast::Operator::NotEqual => !json_values_equal(lhs, rhs),
        ast::Operator::GreaterThan => json_compare(lhs, rhs) == Some(std::cmp::Ordering::Greater),
        ast::Operator::LessThan => json_compare(lhs, rhs) == Some(std::cmp::Ordering::Less),
        ast::Operator::GreaterThanOrEqual => {
            matches!(
                json_compare(lhs, rhs),
                Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)
            )
        }
        ast::Operator::LessThanOrEqual => {
            matches!(
                json_compare(lhs, rhs),
                Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)
            )
        }
        ast::Operator::In => {
            // rhs should be an array, check if lhs is in it
            if let JsonValue::Array(arr) = rhs {
                arr.iter().any(|item| json_values_equal(lhs, item))
            } else {
                false
            }
        }
        ast::Operator::NotIn => {
            // rhs should be an array, check if lhs is NOT in it
            if let JsonValue::Array(arr) = rhs {
                !arr.iter().any(|item| json_values_equal(lhs, item))
            } else {
                true
            }
        }
        ast::Operator::Like => {
            // Simple LIKE pattern matching (SQL LIKE semantics)
            if let (Some(lhs_str), Some(rhs_str)) = (lhs.as_str(), rhs.as_str()) {
                like_pattern_match(lhs_str, rhs_str)
            } else {
                false
            }
        }
        ast::Operator::NotLike => {
            // Simple NOT LIKE pattern matching
            if let (Some(lhs_str), Some(rhs_str)) = (lhs.as_str(), rhs.as_str()) {
                !like_pattern_match(lhs_str, rhs_str)
            } else {
                true
            }
        }
    }
}

/// Compare two JSON values for equality
fn json_values_equal(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Null, JsonValue::Null) => true,
        (JsonValue::Bool(a), JsonValue::Bool(b)) => a == b,
        (JsonValue::Number(a), JsonValue::Number(b)) => {
            // Try to compare as integers first, then floats
            if let (Some(a_i), Some(b_i)) = (a.as_i64(), b.as_i64()) {
                a_i == b_i
            } else if let (Some(a_f), Some(b_f)) = (a.as_f64(), b.as_f64()) {
                (a_f - b_f).abs() < f64::EPSILON
            } else {
                false
            }
        }
        (JsonValue::String(a), JsonValue::String(b)) => a == b,
        (JsonValue::Array(a), JsonValue::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| json_values_equal(x, y))
        }
        (JsonValue::Object(a), JsonValue::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).map_or(false, |w| json_values_equal(v, w)))
        }
        _ => false,
    }
}

/// Compare two JSON values (returns None if types are incomparable)
fn json_compare(a: &JsonValue, b: &JsonValue) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (JsonValue::Null, JsonValue::Null) => Some(std::cmp::Ordering::Equal),
        (JsonValue::Bool(a), JsonValue::Bool(b)) => Some(a.cmp(b)),
        (JsonValue::Number(a), JsonValue::Number(b)) => {
            // Try to compare as integers first, then floats
            if let (Some(a_i), Some(b_i)) = (a.as_i64(), b.as_i64()) {
                Some(a_i.cmp(&b_i))
            } else if let (Some(a_f), Some(b_f)) = (a.as_f64(), b.as_f64()) {
                a_f.partial_cmp(&b_f)
            } else {
                None
            }
        }
        (JsonValue::String(a), JsonValue::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Simple LIKE pattern matching (SQL LIKE semantics: % matches any sequence, _ matches any single character)
/// Uses a simple recursive approach
fn like_pattern_match(text: &str, pattern: &str) -> bool {
    like_pattern_match_recursive(
        text.chars().collect::<Vec<_>>().as_slice(),
        pattern.chars().collect::<Vec<_>>().as_slice(),
    )
}

/// Recursive helper for LIKE pattern matching
fn like_pattern_match_recursive(text: &[char], pattern: &[char]) -> bool {
    match (text.first(), pattern.first()) {
        (None, None) => true,     // Both exhausted - match
        (Some(_), None) => false, // Pattern exhausted but text remains - no match
        (None, Some(&'%')) => {
            // Text exhausted, pattern is % - check if rest of pattern matches empty string
            like_pattern_match_recursive(&[], &pattern[1..])
        }
        (None, Some(_)) => false, // Text exhausted but pattern has non-% - no match
        (Some(_), Some(&'%')) => {
            // Try matching % to zero characters, or one+ characters
            like_pattern_match_recursive(text, &pattern[1..])
                || like_pattern_match_recursive(&text[1..], pattern)
        }
        (Some(_), Some(&'_')) => {
            // _ matches any single character
            like_pattern_match_recursive(&text[1..], &pattern[1..])
        }
        (Some(&t), Some(&p)) if t == p => {
            // Characters match
            like_pattern_match_recursive(&text[1..], &pattern[1..])
        }
        (Some(_), Some(_)) => false, // Characters don't match
    }
}

/// Convert a QueryValue to JSON, resolving session variables
fn query_value_to_json(
    value: &ast::QueryValue,
    session: &HashMap<String, SessionValue>,
) -> JsonValue {
    match value {
        ast::QueryValue::String((_, s)) => JsonValue::String(s.clone()),
        ast::QueryValue::Int((_, i)) => JsonValue::Number((*i as i64).into()),
        ast::QueryValue::Float((_, f)) => {
            JsonValue::Number(serde_json::Number::from_f64(*f as f64).unwrap_or(0.into()))
        }
        ast::QueryValue::Bool((_, b)) => JsonValue::Bool(*b),
        ast::QueryValue::Null(_) => JsonValue::Null,
        ast::QueryValue::Variable((_, var)) => {
            // Check if this is a session variable
            if let Some(session_field) = &var.session_field {
                session
                    .get(session_field)
                    .map(|v| session_value_to_json(v))
                    .unwrap_or(JsonValue::Null)
            } else {
                // Regular variable - not supported in permission evaluation
                JsonValue::Null
            }
        }
        ast::QueryValue::LiteralTypeValue((_, details)) => {
            // For literal type values, we'll represent them as strings for now
            // This might need to be more sophisticated depending on use cases
            JsonValue::String(details.name.clone())
        }
        ast::QueryValue::Fn(_) => {
            // Function calls not supported in permission evaluation
            JsonValue::Null
        }
    }
}

/// Convert a SessionValue to JSON
fn session_value_to_json(value: &SessionValue) -> JsonValue {
    match value {
        SessionValue::Null => JsonValue::Null,
        SessionValue::Integer(i) => JsonValue::Number((*i).into()),
        SessionValue::Real(f) => {
            JsonValue::Number(serde_json::Number::from_f64(*f).unwrap_or(0.into()))
        }
        SessionValue::Text(s) => JsonValue::String(s.clone()),
        SessionValue::Blob(b) => {
            // Convert blob to hex string for JSON
            JsonValue::String(b.iter().map(|x| format!("{:02x}", x)).collect::<String>())
        }
    }
}

/// Calculate which sessions should receive which affected rows based on permissions
pub fn calculate_sync_deltas(
    affected_rows: &[AffectedRow],
    connected_sessions: &[ConnectedSession],
    context: &typecheck::Context,
) -> Result<SyncDeltasResult, SyncDeltasError> {
    let mut result = SyncDeltasResult { deltas: Vec::new() };

    // For each connected session, collect affected rows they should receive
    for session in connected_sessions {
        let mut session_affected_rows = Vec::new();

        for affected_row in affected_rows {
            // Find the table in context
            let table = context
                .tables
                .values()
                .find(|t| {
                    let actual_table_name = ast::get_tablename(&t.record.name, &t.record.fields);
                    actual_table_name == affected_row.table_name
                })
                .ok_or_else(|| SyncDeltasError::TableNotFound(affected_row.table_name.clone()))?;

            // Get select permission for this table
            let permission = ast::get_permissions(&table.record, &ast::QueryOperation::Select);

            // If no permission (public), all sessions can see it
            let should_receive = if let Some(perm) = permission {
                // Convert row JSON to HashMap for easier access
                let row_data = if let JsonValue::Object(obj) = &affected_row.row {
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    return Err(SyncDeltasError::InvalidRowData(
                        "Row data must be a JSON object".to_string(),
                    ));
                };

                // Evaluate permission
                evaluate_permission(&perm, &row_data, &session.session)
            } else {
                // No permission means public - all sessions can see it
                true
            };

            if should_receive {
                session_affected_rows.push(affected_row.clone());
            }
        }

        // Only add delta if there are affected rows for this session
        if !session_affected_rows.is_empty() {
            result.deltas.push(SessionDelta {
                session_id: session.session_id.clone(),
                affected_rows: session_affected_rows,
            });
        }
    }

    Ok(result)
}

#[derive(Debug)]
pub enum SyncDeltasError {
    TableNotFound(String),
    InvalidRowData(String),
}

impl std::fmt::Display for SyncDeltasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncDeltasError::TableNotFound(table) => {
                write!(f, "Table not found: {}", table)
            }
            SyncDeltasError::InvalidRowData(msg) => {
                write!(f, "Invalid row data: {}", msg)
            }
        }
    }
}

impl std::error::Error for SyncDeltasError {}
