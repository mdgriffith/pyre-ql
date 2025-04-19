pub const MIGRATION_TABLE: &str = "_pyre_migrations";

pub const SCHEMA_TABLE: &str = "_pyre_schema";

pub const LIST_MIGRATIONS: &str = "select name from _pyre_migrations;";

// Add this near the top with other constants
pub const GET_SCHEMA: &str = "selct schema from _pyre_schema limit 1;";

//
//
// Creating tables

pub const CREATE_MIGRATION_TABLE: &str = "create table if not exists \"_pyre_migrations\" (
    id integer not null primary key autoincrement,
    createdAt integer not null default (unixepoch()),
    finishedAt integer,
    error text,
    sql text not null,
    schema_diff text not null
);";

pub const CREATE_SCHEMA_TABLE: &str =
    "create table if not exists _pyre_schema (schema TEXT PRIMARY KEY);";

pub const INSERT_MIGRATION: &str = "insert into _pyre_migrations (name) values (?);";
