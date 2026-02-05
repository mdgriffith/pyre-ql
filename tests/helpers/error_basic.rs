#[derive(Debug)]
pub enum TestError {
    Io(std::io::Error),
    Database(libsql::Error),
    ParseError(String),
    TypecheckError(String),
    InvalidPath,
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Io(e) => write!(f, "IO Error: {}", e),
            TestError::Database(e) => write!(f, "Database Error: {}", e),
            TestError::ParseError(s) => write!(f, "{}", s),
            TestError::TypecheckError(s) => write!(f, "{}", s),
            TestError::InvalidPath => write!(f, "Invalid Path"),
        }
    }
}

impl std::error::Error for TestError {}
