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
    Typescript,
}

pub enum Server {
    Typescript,
}

pub fn generate_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    write_client_schema(&Client::Elm, context, database, files);
    write_client_schema(&Client::Typescript, context, database, files);
    write_server_schema(&Server::Typescript, context, database, files)
}

// CLIENT

fn write_client_schema(
    client: &Client,
    context: &typecheck::Context,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Target directory is
    // {base_out_dir}/client/{lang}
    // Use relative path to avoid duplication when joined with base_path in write_generated_files
    let client_dir = Path::new("client");
    let out_dir = to_client_dir_path(client, &client_dir);
    // out_dir is
    // client/{lang} (relative path)
    match client {
        Client::Elm => generate::client::elm::generate(&out_dir, database, files),
        Client::Typescript => {
            generate::client::typescript::generate(context, &out_dir, database, files)
        }
    }
}

// SERVER

fn write_server_schema(
    lang: &Server,
    context: &typecheck::Context,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Target directory is
    // {base_out_dir}/server/{lang}
    // Use relative path to avoid duplication when joined with base_path in write_generated_files
    let server_dir = Path::new("server");
    let out_dir = to_server_dir_path(lang, &server_dir);
    // out_dir is
    // server/{lang} (relative path)
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
        Client::Typescript => out_dir.join("typescript"),
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
        &Client::Typescript,
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
        true, // Generate runner file with all queries
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
    // Create relative path: client/{lang} (relative to base_out_dir)
    let client_dir = Path::new("client");
    let out_dir = to_client_dir_path(client, &client_dir);

    match client {
        Client::Elm => {
            generate::client::elm::generate_queries(&context, &query_list, &out_dir, files)
        }
        Client::Typescript => {
            generate::client::typescript::generate_queries(&context, &query_list, &out_dir, files)
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
    generate_runner_file: bool,
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
                database,
                &out_dir,
                files,
                generate_runner_file,
            )
        }
    }
}
