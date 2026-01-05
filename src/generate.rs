use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::ast;
use crate::generate;
use crate::typecheck;

pub mod client;
pub mod server;
pub mod sql;
pub mod to_string;
pub mod typealias;

pub enum Client {
    Elm,
    Node,
}

pub enum Server {
    Typescript,
}

pub fn generate_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    write_client_schema(&Client::Elm, context, database, base_out_dir, files);
    write_client_schema(&Client::Node, context, database, base_out_dir, files);
    write_server_schema(&Server::Typescript, context, database, base_out_dir, files)
}

// CLIENT

fn write_client_schema(
    client: &Client,
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    let client_dir = base_out_dir.join("client");
    let out_dir = to_client_dir_path(client, &client_dir);
    match client {
        Client::Elm => generate::client::elm::generate(database, files),
        Client::Node => generate::client::node::generate(&out_dir, database, files),
    }
}

// SERVER

fn write_server_schema(
    lang: &Server,
    context: &typecheck::Context,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Target directory is
    // {base_out_dir}/server/{lang}
    let server_dir = base_out_dir.join("server");
    let out_dir = to_server_dir_path(lang, &server_dir);
    // out_dir is
    // {base_out_dir}/server/{lang}
    match lang {
        Server::Typescript => {
            // Server schema
            generate::server::typescript::generate(&context, database, &out_dir, files)
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
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    write_client_queries(
        &Client::Elm,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
        files,
    );
    write_client_queries(
        &Client::Node,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
        files,
    );
    write_server_queries(
        &Server::Typescript,
        context,
        query_list,
        all_query_info,
        database,
        base_out_dir,
        files,
    );
    ()
}

// CLIENT

fn write_client_queries(
    client: &Client,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    base_out_dir: &Path,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    let client_dir = base_out_dir.join("client");
    // filesystem::create_dir_if_not_exists(&client_dir)?;
    let out_dir = to_client_dir_path(client, &client_dir);

    match client {
        Client::Elm => {
            generate::client::elm::generate_queries(&context, &query_list, &out_dir, files)
        }
        Client::Node => {
            generate::client::node::generate_queries(&context, &query_list, &out_dir, files)
        }
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
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Create relative path: server/typescript (relative to base_out_dir)
    let server_dir = Path::new("server");
    let out_dir = to_server_dir_path(lang, &server_dir);
    match lang {
        Server::Typescript => {
            // Server queries
            generate::server::typescript::generate_queries(
                context,
                &all_query_info,
                &query_list,
                &out_dir,
                files,
            )
        }
    }
}
