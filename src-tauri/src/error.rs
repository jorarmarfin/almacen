use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("base de datos no configurada")]
    NotConfigured,
    #[error("el negocio ya fue inicializado")]
    AlreadyInitialized,
    #[error("{0}")]
    Validation(String),
    #[error("error de base de datos: {0}")]
    Db(#[from] sqlx::Error),
    #[error("error de migración: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuración inválida: {0}")]
    Config(#[from] serde_json::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "NOT_CONFIGURED",
            Self::AlreadyInitialized => "ALREADY_INITIALIZED",
            Self::Validation(_) => "VALIDATION",
            Self::Db(_) => "DB",
            Self::Migrate(_) => "MIGRATION",
            Self::Io(_) => "IO",
            Self::Config(_) => "CONFIG",
        }
    }
}

/// Error serializable hacia la UI: `{ code, message }`.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
