use serde_json;
use std::io::{self, Read};

use super::shared::{parse_database_schemas, FileError, Options};
use pyre::error;
use pyre::filesystem;
use pyre::parser;
use pyre::typecheck;

pub fn check(options: &Options, files: Vec<String>, json: bool) -> io::Result<()> {
    match run_check(
        crate::filesystem::collect_filepaths(&options.in_dir)?,
        options.enable_color,
    ) {
        Ok(errors) => {
            let has_errors = !errors.is_empty();
            if json {
                let mut formatted_errors = Vec::new();
                for file_error in errors {
                    for error in &file_error.errors {
                        formatted_errors.push(error::format_json(error));
                    }
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&formatted_errors).unwrap()
                );
            } else {
                for file_error in errors {
                    for err in &file_error.errors {
                        let formatted_error =
                            error::format_error(&file_error.source, err, options.enable_color);
                        eprintln!("{}", formatted_error);
                    }
                }
            }
            if has_errors {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error checking files: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

fn run_check(paths: filesystem::Found, enable_color: bool) -> io::Result<Vec<FileError>> {
    let schema = parse_database_schemas(&paths, enable_color)?;
    let mut all_file_errors = Vec::new();

    match typecheck::check_schema(&schema) {
        Err(errors) => {
            all_file_errors.push(FileError {
                source: "schema.pyre".to_string(),
                errors: errors,
            });
        }
        Ok(mut context) => {
            for query_file_path in paths.query_files {
                let mut file_errors = Vec::new();
                let mut query_file = std::fs::File::open(query_file_path.clone())?;
                let mut query_source_str = String::new();
                query_file.read_to_string(&mut query_source_str)?;

                match parser::parse_query(&query_file_path, &query_source_str) {
                    Ok(query_list) => {
                        context.current_filepath = query_file_path.clone();
                        let typecheck_result = typecheck::check_queries(&query_list, &mut context);

                        match typecheck_result {
                            Ok(_) => {}
                            Err(errors) => {
                                file_errors.extend(errors);
                            }
                        }
                    }
                    Err(err) => {
                        if let Some(parsing_error) = parser::convert_parsing_error(err) {
                            file_errors.push(parsing_error);
                        }
                    }
                }

                if !file_errors.is_empty() {
                    all_file_errors.push(FileError {
                        source: query_source_str,
                        errors: file_errors,
                    });
                }
            }
        }
    }

    Ok(all_file_errors)
}
