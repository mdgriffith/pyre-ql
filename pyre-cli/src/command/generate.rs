use std::fs;
use std::io::{self, Read};
use std::path::Path;

use super::shared::{parse_database_schemas, Options};
use pyre::error;
use pyre::filesystem;
use pyre::generate;
use pyre::parser;
use pyre::typecheck;

pub fn generate(options: &Options, out: &str) -> io::Result<()> {
    execute(
        options,
        filesystem::collect_filepaths(&options.in_dir)?,
        Path::new(out),
    )
}

fn execute(_options: &Options, paths: filesystem::Found, out_dir: &Path) -> io::Result<()> {
    let schema = parse_database_schemas(&paths)?;

    match typecheck::check_schema(&schema) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths);
        }
        Ok(mut context) => {
            // Clear the generated folder
            generate::clear(&out_dir)?;

            // Ensure dir is present
            filesystem::create_dir_if_not_exists(&out_dir)?;

            // Generate schema files
            generate::write_schema(&context, &schema, out_dir)?;

            for query_file_path in paths.query_files {
                let mut query_file = fs::File::open(query_file_path.clone())?;
                let mut query_source_str = String::new();
                query_file.read_to_string(&mut query_source_str)?;

                match parser::parse_query(&query_file_path, &query_source_str) {
                    Ok(query_list) => {
                        // Typecheck and generate
                        context.current_filepath = query_file_path.clone();
                        let typecheck_result =
                            typecheck::check_queries(&schema, &query_list, &mut context);

                        match typecheck_result {
                            Ok(all_query_info) => {
                                generate::write_queries(
                                    &context,
                                    &query_list,
                                    &all_query_info,
                                    &schema,
                                    out_dir,
                                )?;
                            }
                            Err(error_list) => {
                                let mut errors = "".to_string();
                                for err in error_list {
                                    let formatted_error =
                                        error::format_error(&query_source_str, &err);
                                    errors.push_str(&formatted_error);
                                }

                                eprintln!("{}", errors);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("{}", parser::render_error(&query_source_str, err));
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    Ok(())
}
