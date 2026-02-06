use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::ast;
use crate::generate;
use crate::typecheck;

pub mod client;
pub mod server;
pub mod simple;
pub mod sql;
pub mod to_string;
pub mod typealias;
pub mod typescript;

pub enum Client {
    Elm,
}

pub enum Server {
    Typescript,
}

pub fn generate_schema(
    context: &typecheck::Context,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    write_client_schema(&Client::Elm, database, files);
    let typescript_core_dir = Path::new("typescript/core");
    let typescript_dir = Path::new("typescript");
    generate::typescript::core::generate_schema(context, database, typescript_core_dir, files);
    generate::typescript::targets::server::generate_schema(
        context,
        database,
        typescript_dir,
        files,
    );
    generate::typescript::targets::simple::generate_schema(database, typescript_dir, files);
}

// CLIENT

fn write_client_schema(
    client: &Client,
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
    }
}

fn to_server_dir_path(server: &Server, out_dir: &Path) -> PathBuf {
    match server {
        Server::Typescript => out_dir.join("typescript"),
    }
}

// SIMPLE

fn write_simple_schema(
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    let simple_dir = Path::new("simple");
    generate::simple::typescript::generate(database, &simple_dir, files);
}

// WRITE QUERIES

pub fn write_queries(
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    write_client_queries(&Client::Elm, context, query_list, files);
    let typescript_core_dir = Path::new("typescript/core");
    let typescript_dir = Path::new("typescript");
    generate::typescript::core::generate_queries(
        context,
        all_query_info,
        query_list,
        typescript_core_dir,
        files,
    );
    generate::typescript::targets::server::generate_queries(
        context,
        all_query_info,
        query_list,
        typescript_dir,
        files,
    );
    generate::typescript::targets::simple::generate_queries(
        context,
        all_query_info,
        query_list,
        typescript_dir,
        files,
    );
}

// CLIENT

fn write_client_queries(
    client: &Client,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Create relative path: client/{lang} (relative to base_out_dir)
    let client_dir = Path::new("client");
    let out_dir = to_client_dir_path(client, &client_dir);

    match client {
        Client::Elm => {
            generate::client::elm::generate_queries(&context, &query_list, &out_dir, files)
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

// SIMPLE

fn write_simple_queries(
    context: &typecheck::Context,
    query_list: &ast::QueryList,
    all_query_info: &HashMap<String, typecheck::QueryInfo>,
    database: &ast::Database,
    files: &mut Vec<crate::filesystem::GeneratedFile<String>>,
) {
    // Use relative path: simple/ (relative to base_out_dir)
    let simple_dir = Path::new("simple");
    generate::simple::typescript::generate_queries(
        context,
        all_query_info,
        query_list,
        database,
        &simple_dir,
        files,
    );
}
