use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::backend::manga_provider::ProviderError;
use crate::i18n::{is_es, tr};

/// Serializable, thread-safe error type at the public boundary of `manga-core`.
///
/// Designed to cross async Tauri IPC boundaries cleanly (`Send + Sync + Serialize + Deserialize`)
/// and present human-readable ES/EN error messages to end users.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum CoreError {
    Provider(ProviderError),
    Database(String),
    Io(String),
    Config(String),
    InvalidInput(String),
    NotFound(String),
    Other(String),
}

impl CoreError {
    /// Returns a concise, localized message suitable for displaying in a UI alert or toast.
    pub fn user_message(&self) -> String {
        let es = is_es();
        match self {
            Self::Provider(err) => err.user_friendly_message(),
            Self::Database(msg) => {
                if es {
                    format!("Error de base de datos local: {msg}")
                } else {
                    format!("Local database error: {msg}")
                }
            },
            Self::Io(msg) => {
                if es {
                    format!("No se pudo leer o escribir el archivo: {msg}")
                } else {
                    format!("Could not read or write file: {msg}")
                }
            },
            Self::Config(msg) => {
                if es {
                    format!("Error en la configuración: {msg}")
                } else {
                    format!("Configuration error: {msg}")
                }
            },
            Self::InvalidInput(msg) => {
                if es {
                    format!("Datos no válidos: {msg}")
                } else {
                    format!("Invalid input: {msg}")
                }
            },
            Self::NotFound(msg) => {
                if es {
                    format!("No encontrado: {msg}")
                } else {
                    format!("Not found: {msg}")
                }
            },
            Self::Other(msg) => {
                if msg.trim().is_empty() {
                    tr("Ocurrió un error inesperado.", "An unexpected error occurred.").to_string()
                } else {
                    msg.clone()
                }
            },
        }
    }
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.user_message())
    }
}

impl std::error::Error for CoreError {}

impl From<ProviderError> for CoreError {
    fn from(value: ProviderError) -> Self {
        Self::Provider(value)
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<String> for CoreError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}

impl From<&str> for CoreError {
    fn from(value: &str) -> Self {
        Self::Other(value.to_string())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::manga_provider::MangaProviders;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn core_error_is_send_sync_and_serializable() {
        assert_send_sync::<CoreError>();

        let err = CoreError::Provider(ProviderError::CloudflareBlocked {
            provider: Some(MangaProviders::Inmanga),
        });
        let json = serde_json::to_string(&err).expect("CoreError should serialize");
        let parsed: CoreError = serde_json::from_str(&json).expect("CoreError should deserialize");
        assert_eq!(parsed, err);
        assert!(!parsed.user_message().is_empty());

        let db_err: CoreError = rusqlite::Error::InvalidQuery.into();
        assert!(matches!(db_err, CoreError::Database(_)));
        assert!(!db_err.to_string().is_empty());
    }
}
