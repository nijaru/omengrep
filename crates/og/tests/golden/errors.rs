//! Custom error types with context and conversion.

use std::fmt;
use std::io;

/// Application-specific error types.
#[derive(Debug)]
pub enum AppError {
    /// Database operation failed.
    Database(DatabaseError),
    /// Network request failed.
    Network(NetworkError),
    /// Invalid input provided.
    Validation(ValidationError),
    /// Resource not found.
    NotFound { resource: String, id: String },
    /// Internal server error.
    Internal(String),
}

#[derive(Debug)]
pub struct DatabaseError {
    pub operation: String,
    pub table: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

#[derive(Debug)]
pub struct NetworkError {
    pub url: String,
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub value: Option<String>,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(e) => write!(f, "Database error in {}.{}: ", e.table, e.operation),
            AppError::Network(e) => write!(f, "Network error for {}: {}", e.url, e.message),
            AppError::Validation(e) => write!(f, "Validation error for '{}': {}", e.field, e.message),
            AppError::NotFound { resource, id } => write!(f, "{} not found: {}", resource, id),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Database(e) => e.source.as_ref().map(|s| s.as_ref() as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Validation(ValidationError {
            field: "json".to_string(),
            message: err.to_string(),
            value: None,
        })
    }
}

/// Result type alias for AppError.
pub type Result<T> = std::result::Result<T, AppError>;

/// Extension trait for adding context to errors.
pub trait ResultExt<T> {
    fn context(self, msg: &str) -> Result<T>;
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T, E: Into<AppError>> ResultExt<T> for std::result::Result<T, E> {
    fn context(self, msg: &str) -> Result<T> {
        self.map_err(|e| {
            let err = e.into();
            AppError::Internal(format!("{}: {}", msg, err))
        })
    }

    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| {
            let err = e.into();
            AppError::Internal(format!("{}: {}", f(), err))
        })
    }
}

/// Helper to create NotFound errors.
pub fn not_found(resource: &str, id: &str) -> AppError {
    AppError::NotFound {
        resource: resource.to_string(),
        id: id.to_string(),
    }
}

/// Helper to create validation errors.
pub fn validation_error(field: &str, message: &str) -> AppError {
    AppError::Validation(ValidationError {
        field: field.to_string(),
        message: message.to_string(),
        value: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_display() {
        let err = not_found("User", "123");
        assert_eq!(err.to_string(), "User not found: 123");
    }

    #[test]
    fn test_validation_error_display() {
        let err = validation_error("email", "Invalid format");
        assert_eq!(err.to_string(), "Validation error for 'email': Invalid format");
    }
}
