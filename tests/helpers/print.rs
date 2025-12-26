use libsql::Connection;
use pyre::db::diff;
use pyre::db::introspect;

/// Print a database diff in a readable format
pub fn print_db_diff(db_diff: &diff::Diff) {
    eprintln!("=== DB DIFF ===");
    eprintln!(
        "Added tables: {:?}",
        db_diff.added.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    eprintln!(
        "Removed tables: {:?}",
        db_diff.removed.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    eprintln!(
        "Modified records: {:?}",
        db_diff
            .modified_records
            .iter()
            .map(|r| &r.name)
            .collect::<Vec<_>>()
    );
    for record_diff in &db_diff.modified_records {
        eprintln!(
            "  {} changes: {:?}",
            record_diff.name,
            record_diff.changes.len()
        );
        for change in &record_diff.changes {
            eprintln!("    {:?}", change);
        }
    }
}

/// Print an introspection in a readable format
pub fn print_introspection(introspection: &introspect::Introspection) {
    eprintln!("\n=== INTROSPECTION ===");
    eprintln!(
        "Introspection tables: {:?}",
        introspection
            .tables
            .iter()
            .map(|t| &t.name)
            .collect::<Vec<_>>()
    );
    for table in &introspection.tables {
        eprintln!("  Table {}:", table.name);
        for col in &table.columns {
            eprintln!("    Column: {} ({})", col.name, col.column_type);
        }
    }
}

/// Print database contents by querying the database directly
pub async fn print_database_contents(conn: &Connection) -> Result<(), libsql::Error> {
    eprintln!("\n=== DATABASE CONTENTS ===");
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_pyre_%'",
            (),
        )
        .await?;
    let mut table_names = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(0)?;
        table_names.push(name);
    }
    eprintln!("Tables in database: {:?}", table_names);
    for table_name in &table_names {
        let mut col_rows = conn
            .query(&format!("PRAGMA table_info(\"{}\")", table_name), ())
            .await?;
        eprintln!("  Table {} columns:", table_name);
        while let Some(col_row) = col_rows.next().await? {
            let _cid: i32 = col_row.get(0)?;
            let name: String = col_row.get(1)?;
            let col_type: String = col_row.get(2)?;
            let notnull: i32 = col_row.get(3)?;
            let pk: i32 = col_row.get(5)?;
            eprintln!(
                "    {}: {} (notnull: {}, pk: {})",
                name, col_type, notnull, pk
            );
        }
    }
    Ok(())
}
