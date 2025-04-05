use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use super::shared::{
    get_stdin, parse_database_schemas, parse_single_schema, parse_single_schema_from_source,
    write_db_schema, write_schema, Options,
};
use crate::filesystem;
use pyre::ast;
use pyre::format;
use pyre::generate;
use pyre::parser;

pub fn format(options: &Options, files: &Vec<String>, to_stdout: bool) -> io::Result<()> {
    match files.len() {
        0 => match get_stdin()? {
            Some(stdin) => {
                let paths = filesystem::collect_filepaths(&options.in_dir)?;
                let schema = parse_database_schemas(&paths)?;

                // We're assuming this file is a query because we don't have a filepath
                format_query_to_std_out(&options, &schema, &stdin)?;
            }
            None => {
                println!("Formatting all files in {}", options.in_dir.display());
                format_all(&options, filesystem::collect_filepaths(&options.in_dir)?)?;
            }
        },
        1 => {
            let file_path = &files[0];

            match get_stdin()? {
                Some(stdin) => {
                    if pyre::filesystem::is_schema_file(file_path) {
                        let mut schema = parse_single_schema_from_source(file_path, &stdin)?;
                        format::schema(&mut schema);
                        // Always write to stdout if stdin is provided
                        write_schema(&options, &true, &schema)?;
                    } else {
                        let paths = filesystem::collect_filepaths(&options.in_dir)?;
                        let schema = parse_database_schemas(&paths)?;

                        format_query_to_std_out(&options, &schema, &stdin)?;
                    }
                }
                None => {
                    if pyre::filesystem::is_schema_file(file_path) {
                        let mut schema = parse_single_schema(file_path)?;
                        format::schema(&mut schema);
                        write_schema(&options, &to_stdout, &schema)?;
                    } else {
                        let paths = filesystem::collect_filepaths(&options.in_dir)?;
                        let database = parse_database_schemas(&paths)?;

                        format_query(&options, &database, &to_stdout, file_path)?;
                    }
                }
            }
        }
        _ => {
            for file_path in files {
                if !file_path.ends_with(".pyre") && !to_stdout {
                    println!("{} doesn't end in .pyre, skipping", file_path);
                    continue;
                }

                if pyre::filesystem::is_schema_file(&file_path) {
                    let mut schema = parse_single_schema(&file_path)?;
                    format::schema(&mut schema);
                    write_schema(&options, &to_stdout, &schema)?;
                } else {
                    let paths = filesystem::collect_filepaths(&options.in_dir)?;
                    let database = parse_database_schemas(&paths)?;

                    format_query(&options, &database, &to_stdout, &file_path)?;
                }
            }
            if !to_stdout {
                println!("{} files were formatted", files.len());
            }
        }
    }
    Ok(())
}

fn format_all(options: &Options, paths: pyre::filesystem::Found) -> io::Result<()> {
    let mut database = parse_database_schemas(&paths)?;

    format::database(&mut database);
    write_db_schema(options, &database)?;

    // Format queries
    for query_file_path in paths.query_files {
        format_query(&options, &database, &false, &query_file_path)?;
    }

    Ok(())
}

fn format_query_to_std_out(
    _options: &Options,
    database: &ast::Database,
    query_source_str: &str,
) -> io::Result<()> {
    match parser::parse_query("stdin", query_source_str) {
        Ok(mut query_list) => {
            // Format query
            format::query_list(database, &mut query_list);

            // Convert to string
            let formatted = generate::to_string::query(&query_list);

            println!("{}", formatted);
            return Ok(());
        }
        Err(_) => {
            println!("{}", query_source_str);
            return Ok(());
        }
    }
}

fn format_query(
    _options: &Options,
    database: &ast::Database,
    to_stdout: &bool,
    query_file_path: &str,
) -> io::Result<()> {
    let mut query_file = fs::File::open(query_file_path)?;
    let mut query_source_str = String::new();
    query_file.read_to_string(&mut query_source_str)?;

    match parser::parse_query(query_file_path, &query_source_str) {
        Ok(mut query_list) => {
            // Format query
            format::query_list(database, &mut query_list);

            // Convert to string
            let formatted = generate::to_string::query(&query_list);
            if *to_stdout {
                println!("{}", formatted);
                return Ok(());
            }
            let path = Path::new(&query_file_path);
            let mut output = fs::File::create(path).expect("Failed to create file");
            output
                .write_all(formatted.as_bytes())
                .expect("Failed to write to file");
        }
        Err(err) => eprintln!("{}", parser::render_error(&query_source_str, err)),
    }

    Ok(())
}
