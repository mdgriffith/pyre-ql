use std::io::{self};
use std::path::Path;

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

fn write_client(client: &Client, database: &ast::Database, out_dir: &Path) -> io::Result<()> {
    match client {
        Client::Elm => generate::elm::write(out_dir, database),
    }
}

fn write_server(
    lang: &Server,
    context: &typecheck::Context,
    database: &ast::Database,
    out_dir: &Path,
) -> io::Result<()> {
    match lang {
        Server::Typescript => generate::typescript::write(&context, database, out_dir),
    }
}
