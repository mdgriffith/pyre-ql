use crate::ast::{self, WhereArg};
use crate::sync::SessionValue;
use crate::typecheck;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// Sync deltas module requires json feature for JSON value handling
#[cfg(feature = "json")]
use serde_json::{Map, Value as JsonValue};

// When json feature is not enabled, sync deltas functionality is not available
#[cfg(not(feature = "json"))]
compile_error!("sync_deltas module requires the 'json' feature to be enabled");

/// A single affected row from a mutation
#[derive(Clone, Serialize, Deserialize)]
pub struct AffectedRow {
    pub table_name: String,
    pub row: JsonValue,       // Object with column names as keys
    pub headers: Vec<String>, // Column names in order
}

/// A connected session with its identifier and values
#[derive(Clone)]
pub struct ConnectedSession {
    pub session_id: String,
    pub session: HashMap<String, SessionValue>,
}

/// A group of sessions that share the same affected row indices
#[derive(Serialize, Deserialize)]
pub struct AffectedRowGroup {
    pub session_ids: HashSet<String>,
    pub affected_row_indices: Vec<usize>, // indices into all_affected_rows
}

/// Result containing deltas with deduplicated affected rows
#[derive(Serialize, Deserialize)]
pub struct SyncDeltasResult {
    /// Shared pool of all unique affected rows
    pub all_affected_rows: Vec<AffectedRow>,
    /// Groups of sessions, each referencing rows by index
    pub groups: Vec<AffectedRowGroup>,
}

/// Evaluate a permission WhereArg against row data and session values
/// Returns true if the permission condition is satisfied
///
/// Optimized to work directly with JsonValue::Object references, avoiding HashMap conversion
fn evaluate_permission(
    where_arg: &WhereArg,
    row_data: &Map<String, JsonValue>,
    session: &HashMap<String, SessionValue>,
) -> bool {
    match where_arg {
        WhereArg::Column(is_session_var, fieldname, op, value, _field_name_range) => {
            // Get the right-hand side value first (needed for both paths)
            let rhs_value = query_value_to_json(value, session);

            // Get the left-hand side value
            // Optimized: for row columns, use reference directly (no clone!)
            // For session variables, conversion to JsonValue is unavoidable but less frequent
            if *is_session_var {
                // Session variable - convert to JsonValue (unavoidable conversion)
                let lhs_value = session
                    .get(fieldname)
                    .map_or(JsonValue::Null, |v| session_value_to_json(v));
                evaluate_operator(op, &lhs_value, &rhs_value)
            } else {
                // Table column - use reference directly from Map (no clone!)
                // This is the hot path - most permission checks are on row columns
                let lhs_value_ref = row_data.get(fieldname).unwrap_or(&JsonValue::Null);
                evaluate_operator(op, lhs_value_ref, &rhs_value)
            }
        }
        WhereArg::And(args) => {
            // All conditions must be true - short-circuit on first false
            args.iter()
                .all(|arg| evaluate_permission(arg, row_data, session))
        }
        WhereArg::Or(args) => {
            // At least one condition must be true - short-circuit on first true
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

/// Optimized LIKE pattern matching using byte slices (no allocations)
/// SQL LIKE semantics: % matches any sequence, _ matches any single character
fn like_pattern_match(text: &str, pattern: &str) -> bool {
    like_pattern_match_bytes(text.as_bytes(), pattern.as_bytes(), 0, 0)
}

/// Iterative LIKE pattern matching using byte indices (no allocations)
fn like_pattern_match_bytes(
    text: &[u8],
    pattern: &[u8],
    mut text_idx: usize,
    mut pattern_idx: usize,
) -> bool {
    // Use a stack to handle backtracking for % wildcards
    // Stack stores (text_idx, pattern_idx) pairs
    let mut stack: Vec<(usize, usize)> = Vec::new();

    loop {
        // Check if we've exhausted both text and pattern
        if pattern_idx >= pattern.len() {
            if text_idx >= text.len() {
                return true; // Both exhausted - match
            }
            // Pattern exhausted but text remains - try backtracking
            if let Some((t_idx, p_idx)) = stack.pop() {
                text_idx = t_idx;
                pattern_idx = p_idx;
                continue;
            }
            return false; // No more backtracking options
        }

        if text_idx >= text.len() {
            // Text exhausted
            if pattern[pattern_idx] == b'%' {
                // % can match zero characters
                pattern_idx += 1;
                continue;
            }
            // Try backtracking
            if let Some((t_idx, p_idx)) = stack.pop() {
                text_idx = t_idx;
                pattern_idx = p_idx;
                continue;
            }
            return false;
        }

        match pattern[pattern_idx] {
            b'%' => {
                // % matches zero or more characters
                // Try matching zero characters first (greedy)
                pattern_idx += 1;
                // Also push backtrack point: try matching one+ characters
                stack.push((text_idx + 1, pattern_idx - 1));
            }
            b'_' => {
                // _ matches any single character
                text_idx += 1;
                pattern_idx += 1;
            }
            c => {
                if text[text_idx] == c {
                    // Characters match
                    text_idx += 1;
                    pattern_idx += 1;
                } else {
                    // Characters don't match - try backtracking
                    if let Some((t_idx, p_idx)) = stack.pop() {
                        text_idx = t_idx;
                        pattern_idx = p_idx;
                        continue;
                    }
                    return false; // No match and no backtracking
                }
            }
        }
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
                    .map_or(JsonValue::Null, |v| session_value_to_json(v))
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
            // Convert blob to hex string for JSON without using format!
            // This avoids pulling in std::fmt infrastructure
            const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
            let mut hex = String::with_capacity(b.len() * 2);
            for byte in b {
                hex.push(HEX_CHARS[(byte >> 4) as usize] as char);
                hex.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
            }
            JsonValue::String(hex)
        }
    }
}

/// Calculate which sessions should receive which affected rows based on permissions
/// Optimized with:
/// - Table lookup map (O(1) instead of O(n) per lookup)
/// - Pre-processed row data (convert JSON once)
/// - Grouped sessions by shared affected rows (deduplication)
pub fn calculate_sync_deltas(
    affected_rows: &[AffectedRow],
    connected_sessions: &[ConnectedSession],
    context: &typecheck::Context,
) -> Result<SyncDeltasResult, SyncDeltasError> {
    // OPTIMIZATION 1: Build table lookup map once (O(k) instead of O(n*m*k))
    let mut table_map: HashMap<String, Option<WhereArg>> = HashMap::new();
    for table in context.tables.values() {
        let actual_table_name = ast::get_tablename(&table.record.name, &table.record.fields);
        let permission = ast::get_permissions(&table.record, &ast::QueryOperation::Query);
        table_map.insert(actual_table_name, permission);
    }

    // OPTIMIZATION 2: Extract references to JSON objects (avoid HashMap conversion)
    // Store references to the Map directly - serde_json::Map already supports O(1) lookups
    let processed_rows: Vec<(&Map<String, JsonValue>, &AffectedRow)> = affected_rows
        .iter()
        .map(|row| {
            if let JsonValue::Object(obj) = &row.row {
                Ok((obj, row))
            } else {
                Err(SyncDeltasError::InvalidRowData(
                    "Row data must be a JSON object".to_string(),
                ))
            }
        })
        .collect::<Result<_, _>>()?;

    // OPTIMIZATION 3: Group sessions by shared affected row sets
    // Map: (sorted row indices) -> set of session IDs
    let mut row_set_to_sessions: HashMap<Vec<usize>, HashSet<String>> = HashMap::new();

    for session in connected_sessions {
        let mut session_row_indices = Vec::new();

        for (idx, (row_data, affected_row)) in processed_rows.iter().enumerate() {
            // OPTIMIZATION: Use hash map lookup instead of linear search
            let permission = table_map
                .get(&affected_row.table_name)
                .ok_or_else(|| SyncDeltasError::TableNotFound(affected_row.table_name.clone()))?
                .as_ref();

            // If no permission (public), all sessions can see it
            let should_receive = if let Some(perm) = permission {
                evaluate_permission(perm, row_data, &session.session)
            } else {
                true // Public - all sessions can see it
            };

            if should_receive {
                session_row_indices.push(idx);
            }
        }

        if !session_row_indices.is_empty() {
            // Sort to ensure consistent key for grouping
            session_row_indices.sort_unstable();
            row_set_to_sessions
                .entry(session_row_indices)
                .or_insert_with(HashSet::new)
                .insert(session.session_id.clone());
        }
    }

    // Convert to result structure
    // All affected rows are stored once in the shared pool
    let all_affected_rows = affected_rows.to_vec();

    let groups: Vec<AffectedRowGroup> = row_set_to_sessions
        .into_iter()
        .map(|(row_indices, session_ids)| AffectedRowGroup {
            session_ids,
            affected_row_indices: row_indices,
        })
        .collect();

    Ok(SyncDeltasResult {
        all_affected_rows,
        groups,
    })
}

pub enum SyncDeltasError {
    TableNotFound(String),
    InvalidRowData(String),
}

// Display and Error traits removed to avoid formatting infrastructure
// Errors are converted to strings manually in WASM code
