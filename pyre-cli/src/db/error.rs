pub fn format_libsql_error(e: &libsql::Error) -> String {
    match e {
        libsql::Error::ConnectionFailed(s) => {
            pyre::error::format_custom_error("Connection Failed", s)
        }
        libsql::Error::SqliteFailure(_, s) => pyre::error::format_custom_error("SQLite Failure", s),
        libsql::Error::NullValue => {
            pyre::error::format_custom_error("Null Value", "Null value encountered")
        }
        libsql::Error::Misuse(s) => pyre::error::format_custom_error("API Misuse", s),
        libsql::Error::ExecuteReturnedRows => {
            pyre::error::format_custom_error("Execute Returned Rows", "Execute returned rows")
        }
        libsql::Error::QueryReturnedNoRows => {
            pyre::error::format_custom_error("Query Returned No Rows", "Query returned no rows")
        }
        libsql::Error::InvalidColumnName(s) => {
            pyre::error::format_custom_error("Invalid Column Name", s)
        }
        libsql::Error::ToSqlConversionFailure(e) => {
            pyre::error::format_custom_error("SQL Conversion Failure", &format!("{}", e))
        }
        libsql::Error::SyncNotSupported(s) => {
            pyre::error::format_custom_error("Sync Not Supported", s)
        }
        libsql::Error::ColumnNotFound(_) => {
            pyre::error::format_custom_error("Column Not Found", "Column not found")
        }
        libsql::Error::Hrana(e) => pyre::error::format_custom_error("Hrana", &format!("{}", e)),
        libsql::Error::WriteDelegation(e) => {
            pyre::error::format_custom_error("Write Delegation", &format!("{}", e))
        }
        libsql::Error::Bincode(e) => pyre::error::format_custom_error("Bincode", &format!("{}", e)),
        libsql::Error::InvalidColumnIndex => {
            pyre::error::format_custom_error("Invalid Column Index", "Invalid column index")
        }
        libsql::Error::InvalidColumnType => {
            pyre::error::format_custom_error("Invalid Column Type", "Invalid column type")
        }
        libsql::Error::Sqlite3SyntaxError(_, _, s) => {
            pyre::error::format_custom_error("SQLite3 Syntax Error", s)
        }
        libsql::Error::Sqlite3UnsupportedStatement => pyre::error::format_custom_error(
            "SQLite3 Unsupported Statement",
            "Unsupported statement",
        ),
        libsql::Error::Sqlite3ParserError(e) => {
            pyre::error::format_custom_error("SQLite3 Parser Error", &format!("{}", e))
        }
        libsql::Error::RemoteSqliteFailure(_, _, s) => {
            pyre::error::format_custom_error("Remote SQLite Failure", s)
        }
        libsql::Error::Replication(e) => {
            pyre::error::format_custom_error("Replication", &format!("{}", e))
        }
        libsql::Error::InvalidUTF8Path => {
            pyre::error::format_custom_error("Invalid UTF-8 Path", "Path has invalid UTF-8")
        }
        libsql::Error::FreezeNotSupported(s) => {
            pyre::error::format_custom_error("Freeze Not Supported", s)
        }
        libsql::Error::InvalidParserState(s) => {
            pyre::error::format_custom_error("Invalid Parser State", s)
        }
        libsql::Error::InvalidTlsConfiguration(e) => {
            pyre::error::format_custom_error("Invalid TLS Configuration", &format!("{}", e))
        }
    }
}
