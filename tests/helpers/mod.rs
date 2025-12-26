pub mod print;
pub mod schema;
pub mod test_database;

#[derive(Debug)]
pub enum TestError {
    Io(std::io::Error),
    Database(libsql::Error),
    ParseError(String),
    TypecheckError(String),
    InvalidPath,
    NoQueryFound,
    NoQueryInfoFound,
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Io(e) => write!(f, "IO Error: {}", e),
            TestError::Database(e) => write!(f, "Database Error: {}", e),
            TestError::ParseError(s) => write!(f, "{}", s),
            TestError::TypecheckError(s) => write!(f, "{}", s),
            TestError::InvalidPath => write!(f, "Invalid Path"),
            TestError::NoQueryFound => write!(f, "No Query Found"),
            TestError::NoQueryInfoFound => write!(f, "No Query Info Found"),
        }
    }
}

impl std::error::Error for TestError {}

/// Helper function to convert a Result to a test result, printing errors with proper formatting
pub fn expect_ok<T>(result: Result<T, TestError>) -> T {
    match result {
        Ok(val) => val,
        Err(e) => {
            eprintln!("\n{}", e);
            panic!("Test failed with error (see above for details)");
        }
    }
}
