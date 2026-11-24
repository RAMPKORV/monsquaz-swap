use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, to_value};

use crate::db::DbError;

/// Store a serde enum as its plain snake_case string. Reject non-string values.
pub fn to_db<T: Serialize>(value: &T) -> Result<String, DbError> {
    match to_value(value)? {
        Value::String(value) => Ok(value),
        other => Err(DbError::InvalidEnum(other.to_string())),
    }
}

/// Read a plain string back into a serde enum.
pub fn from_db<T: DeserializeOwned>(value: &str) -> Result<T, DbError> {
    Ok(serde_json::from_value(Value::String(value.to_owned()))?)
}
