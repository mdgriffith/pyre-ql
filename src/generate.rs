use std::collections::HashMap;
use std::io::{self};
use std::path::Path;
use std::path::PathBuf;

use crate::ast;
use crate::filesystem;
use crate::generate;
use crate::typecheck;

pub mod client;
pub mod migration;
pub mod server;
pub mod sql;
pub mod to_string;

pub enum Client {
    Elm,
    Node,
}

pub enum Server {
    Typescript,
}

pub fn write_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    write_client_schema(&Client::Elm, context, database, base_out_dir)?;
    write_client_schema(&Client::Node, context, database, base_out_dir)?;
    write_server_schema(&Server::Typescript, context, database, base_out_dir)
}

pub fn clear(path: &Path) -> io::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

// CLIENT

fn write_client_schema(
    client: &Client,
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    let client_dir = base_out_dir.join("client");
    filesystem::create_dir_if_not_exists(&client_dir)?;
    let out_dir = to_client_dir_path(client, &client_dir);
    match client {
        Client::Elm => generate::client::elm::write(&out_dir, database),
        Client::Node => generate::client::node::write(&out_dir, database),
    }
}

// SERVER

fn write_server_schema(
    lang: &Server,
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    // Target directory is
    // {base_out_dir}/server/{lang}
    let server_dir = base_out_dir.join("server");
    filesystem::create_dir_if_not_exists(&server_dir)?;
    let out_dir = to_server_dir_path(lang, &server_dir);
    // out_dir is
    // {base_out_dir}/server/{lang}
    match lang {
        Server::Typescript => {
            // Server schema
            generate::server::typescript::write(&context, database, &out_dir)
        }
    }
}

fn to_client_dir_path(client: &Client, out_dir: &Path) -> PathBuf {
    match client {
        Client::Elm => out_dir.join("elm"),
        Client::Node => out_dir.join("node"),
    }
}

fn to_server_dir_path(server: &Server, out_dir: &Path) -> PathBuf {
    match server {
        Server::Typescript => out_dir.join("typescript"),
    }
}

// WRITE QUERIES

pub fn write_queries(
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    write_client_queries(
        &Client::Elm,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
    )?;
    write_client_queries(
        &Client::Node,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
    )?;
    write_server_queries(
        &Server::Typescript,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
    )
}

// CLIENT

fn write_client_queries(
    client: &Client,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    let client_dir = base_out_dir.join("client");
    filesystem::create_dir_if_not_exists(&client_dir)?;
    let out_dir = to_client_dir_path(client, &client_dir);

    match client {
        Client::Elm => generate::client::elm::write_queries(&out_dir, &context, &query_list),
        Client::Node => generate::client::node::write_queries(&out_dir, &context, &query_list),
    }
}

// SERVER

fn write_server_queries(
    lang: &Server,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    base_out_dir: &Path,
) -> io::Result<()> {
    let server_dir = base_out_dir.join("server");
    filesystem::create_dir_if_not_exists(&server_dir)?;
    let out_dir = to_server_dir_path(lang, &server_dir);
    match lang {
        Server::Typescript => {
            // Server queries
            generate::server::typescript::write_queries(
                &out_dir,
                context,
                &all_query_info,
                &query_list,
            )
        }
    }
}
