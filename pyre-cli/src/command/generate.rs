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
        crate::filesystem::collect_filepaths(&options.in_dir)?,
        Path::new(out),
    )
}

fn clear(path: &Path) -> io::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn execute(options: &Options, paths: filesystem::Found, out_dir: &Path) -> io::Result<()> {
    let schema = parse_database_schemas(&paths, options.enable_color)?;

    // Build a map of schema filepaths to their contents for error formatting
    let mut schema_file_contents: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_, schema_files) in paths.schema_files.iter() {
        for schema_file in schema_files.iter() {
            schema_file_contents.insert(schema_file.path.clone(), schema_file.content.clone());
        }
    }

    match typecheck::check_schema(&schema) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths, options.enable_color);
        }
        Ok(mut context) => {
            // Clear the generated folder
            clear(&out_dir)?;

            // Ensure dir is present
            crate::filesystem::create_dir_if_not_exists(&out_dir)?;

            let mut files: Vec<filesystem::GeneratedFile<String>> = Vec::new();

            // Generate schema files
            generate::generate_schema(&context, &schema, out_dir, &mut files);

            for query_file_path in paths.query_files {
                let mut query_file = fs::File::open(query_file_path.clone())?;
                let mut query_source_str = String::new();
                query_file.read_to_string(&mut query_source_str)?;

                match parser::parse_query(&query_file_path, &query_source_str) {
                    Ok(query_list) => {
                        // Typecheck and generate
                        context.current_filepath = query_file_path.clone();
                        let typecheck_result = typecheck::check_queries(&query_list, &context);

                        match typecheck_result {
                            Ok(all_query_info) => {
                                generate::write_queries(
                                    &context,
                                    &query_list,
                                    &all_query_info,
                                    &schema,
                                    out_dir,
                                    &mut files,
                                );
                            }
                            Err(error_list) => {
                                let mut errors = "".to_string();
                                for err in error_list {
                                    // Use schema file contents if error is from a schema file, otherwise use query file contents
                                    let source = schema_file_contents
                                        .get(&err.filepath)
                                        .map(|s| s.as_str())
                                        .unwrap_or(&query_source_str);
                                    let formatted_error =
                                        error::format_error(source, &err, options.enable_color);
                                    errors.push_str(&formatted_error);
                                }

                                eprintln!("{}", errors);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "{}",
                            parser::render_error(&query_source_str, err, options.enable_color)
                        );
                        std::process::exit(1);
                    }
                }
            }

            crate::filesystem::write_generated_files(out_dir, files)?;
        }
    }

    Ok(())
}
