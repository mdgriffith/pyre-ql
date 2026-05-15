use crate::sync::SyncPageResult;

pub type DatabaseId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseIdError {
    label: String,
}

impl DatabaseIdError {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
        }
    }
}

impl std::fmt::Display for DatabaseIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is required", self.label)
    }
}

impl std::error::Error for DatabaseIdError {}

pub fn require_database_id(value: impl AsRef<str>) -> Result<DatabaseId, DatabaseIdError> {
    require_database_id_with_label(value, "databaseId")
}

pub fn require_database_id_with_label(
    value: impl AsRef<str>,
    label: &str,
) -> Result<DatabaseId, DatabaseIdError> {
    let value = value.as_ref();
    if value.trim().is_empty() {
        return Err(DatabaseIdError::new(label));
    }

    Ok(value.to_string())
}

pub fn with_database_id(
    database_id: impl AsRef<str>,
    mut result: SyncPageResult,
) -> Result<SyncPageResult, DatabaseIdError> {
    result.database_id = Some(require_database_id(database_id)?);
    Ok(result)
}
