use std::io::{self};
use std::path::Path;
use std::path::PathBuf;

use crate::ast;
use crate::generate;
use crate::typecheck;

pub mod elm;
pub mod migration;
pub mod sql;
pub mod to_string;
pub mod typescript;

pub enum Client {
    Elm,
}

pub enum Server {
    Typescript,
}

pub fn write_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    out_dir: &Path,
) -> io::Result<()> {
    write_client(&Client::Elm, database, out_dir)?;
    write_server(&Server::Typescript, context, database, out_dir)
}

fn clear(path: &Path) -> io::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn write_client(client: &Client, database: &ast::Database, base_out_dir: &Path) -> io::Result<()> {
    let out_dir = to_client_dir_path(client, base_out_dir);
    clear(&out_dir)?;
    match client {
        Client::Elm => generate::elm::write(&out_dir, database),
    }
}

fn write_server(
    lang: &Server,
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    let out_dir = to_server_dir_path(lang, base_out_dir);
    clear(&out_dir)?;
    match lang {
        Server::Typescript => generate::typescript::write(&context, database, &out_dir),
    }
}

fn to_client_dir_path(client: &Client, out_dir: &Path) -> PathBuf {
    match client {
        Client::Elm => out_dir.join("elm"),
    }
}

fn to_server_dir_path(server: &Server, out_dir: &Path) -> PathBuf {
    match server {
        Server::Typescript => out_dir.join("typescript"),
    }
}
