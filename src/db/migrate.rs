pub const MIGRATION_TABLE: &str = "_pyre_migrations";

pub const SCHEMA_TABLE: &str = "_pyre_schema";

pub const LIST_MIGRATIONS: &str = "select name from _pyre_migrations";

//
//
// Creating tables

pub const CREATE_MIGRATION_TABLE: &str = "create table if not exists _pyre_migrations (
    id integer not null primary key autoincrement,
    created_at integer not null default (unixepoch()),
    name text not null,
    finished_at integer,
    error text,
    sql text not null
)";

pub const CREATE_SCHEMA_TABLE: &str = "create table if not exists _pyre_schema (
    id integer not null primary key autoincrement,
    created_at integer not null default (unixepoch()),
    schema text not null
)";

pub const INSERT_MIGRATION_ERROR: &str =
    "insert into _pyre_migrations (name, sql, error) values (?, ?, ?)";

pub const INSERT_MIGRATION_SUCCESS: &str =
    "insert into _pyre_migrations (name, sql, finished_at) values (?, ?, unixepoch())";

pub const INSERT_SCHEMA: &str = "insert into _pyre_schema (schema) values (?)";
