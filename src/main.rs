#![allow(warnings)]
use chrono;
use clap::{Parser, Subcommand};
use colored::*;
use generate::migration;
use serde_json;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use tokio;
use walkdir::WalkDir;

mod ast;
mod db;
mod diff;
mod ext;
mod format;
mod generate;
mod parser;
mod typecheck;

#[derive(Parser)]
#[command(name = "pyre")]
#[command(about = "A CLI tool for pyre operations", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// The input directory to read from (when no subcommand is provided)
    #[arg(long, global = true)]
    r#in: Option<String>,

    /// The output directory to write to (when no subcommand is provided)
    #[arg(long, global = true)]
    out: Option<String>,

    #[arg(long, global = true)]
    migrations: Option<String>,
}

#[derive(Subcommand)]
enum MigrationOperation {
    Generate {
        #[arg(long)]
        name: String,

        #[arg(long)]
        db: String,
    },
    Apply {
        /// The database connection string
        #[arg(long)]
        db: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Format files
    Format {
        /// The input directory to read from
        // #[arg(long)]
        // r#in: String,
        #[arg(required = false)]
        files: Vec<String>,
    },
    // Introspect current db
    Introspect {
        #[arg(long)]
        db: String,
    },
    Migrate {
        #[arg(long)]
        db: String,
    },
    // Generate or run a migration
    Migration {
        #[command(subcommand)]
        operation: MigrationOperation,
    },
}

fn out(options: &Options, file: &str) -> String {
    format!("{}/{}", options.out_dir, file)
}

fn generate_elm_schema(options: &Options, schema: &ast::Schema) -> io::Result<()> {
    create_dir_if_not_exists(&out(options, "elm"));
    create_dir_if_not_exists(&out(options, "elm/Db"));

    let formatted_elm = generate::elm::schema(&schema);

    // Top level Elm files
    let elm_db_path = out(options, "elm/Db.elm");
    let elm_file = Path::new(&elm_db_path);
    let mut output = fs::File::create(elm_file).expect("Failed to create file");
    output
        .write_all(formatted_elm.as_bytes())
        .expect("Failed to write to file");

    // Elm Decoders
    let elm_db_decode_path = out(options, "elm/Db/Decode.elm");
    let elm_decoders = generate::elm::to_schema_decoders(&schema);
    let elm_decoder_file = Path::new(&elm_db_decode_path);
    let mut output = fs::File::create(elm_decoder_file).expect("Failed to create file");
    output
        .write_all(elm_decoders.as_bytes())
        .expect("Failed to write to file");

    // Elm Encoders
    let elm_db_encode_path = out(options, "elm/Db/Encode.elm");
    let elm_encoders = generate::elm::to_schema_encoders(&schema);
    let elm_encoder_file = Path::new(&elm_db_encode_path);
    let mut output = fs::File::create(elm_encoder_file).expect("Failed to create file");
    output
        .write_all(elm_encoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

fn generate_typescript_schema(options: &Options, schema: &ast::Schema) -> io::Result<()> {
    let formatted_ts = generate::typescript::schema(&schema);

    create_dir_if_not_exists(&out(options, "typescript"));
    create_dir_if_not_exists(&out(options, "typescript/db"));

    // Top level TS files
    let ts_db_path = out(options, "typescript/db.ts");
    let ts_file = Path::new(&ts_db_path);
    let mut output = fs::File::create(ts_file).expect("Failed to create file");
    output
        .write_all(formatted_ts.as_bytes())
        .expect("Failed to write to file");

    // TS Decoders
    let ts_db_decoder_path = out(options, "typescript/db/decode.ts");
    let ts_decoders = generate::typescript::to_schema_decoders(&schema);
    let ts_decoder_file = Path::new(&ts_db_decoder_path);
    let mut output = fs::File::create(ts_decoder_file).expect("Failed to create file");
    output
        .write_all(ts_decoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

#[derive(Debug)]
struct Options {
    in_dir: String,
    out_dir: String,
    migration_dir: String,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let options = Options {
        in_dir: cli.r#in.unwrap_or_else(|| "pyre".to_string()),
        out_dir: cli.out.unwrap_or_else(|| "generated".to_string()),
        migration_dir: cli.migrations.unwrap_or_else(|| "migrations".to_string()),
    };

    match &cli.command {
        Some(Commands::Format { files }) => match files.len() {
            0 => {
                println!("Formatting all files in {}", options.in_dir);
                format_all(&options, collect_filepaths(&options.in_dir));
            }
            _ => {
                println!("Formatting files: {:?}", files);

                for file_path in files {
                    if !file_path.ends_with(".pyre") {
                        println!("{} doesn't end in .pyre, skipping", file_path);
                        continue;
                    }

                    if is_schema_file(file_path) {
                        format_schema(&options, file_path);
                    } else {
                        format_query(&options, file_path);
                    }
                }
            }
        },
        Some(Commands::Introspect { db }) => {
            let maybeConn = db::local(db).await;
            match maybeConn {
                Ok(conn) => {
                    let introspection_result = db::introspect(&conn).await;
                    match introspection_result {
                        Ok(introspection) => {
                            println!("Writing Schema");
                            write_schema("./found.schema.pyre", &introspection.schema);
                        }
                        Err(e) => {
                            println!("Failed to connect to database: {:?}", e);
                        }
                    }

                    // println!("{:?}", introspection);

                    // generate_elm_schema(&options, &schema).expect("Failed to generate Elm schema");
                    // generate_typescript_schema(&options, &schema)
                    // .expect("Failed to generate TS schema");
                }
                Err(e) => {
                    println!("Failed to connect to database: {:?}", e);
                }
            }
        }
        Some(Commands::Migrate { db }) => {
            let maybeConn = db::local(db).await;
            match maybeConn {
                Ok(conn) => {
                    let migration_result = db::migrate(&conn, &options.migration_dir).await;
                    match migration_result {
                        Ok(()) => {
                            println!("Migration finished!");
                        }
                        Err(e) => {
                            println!("Failed to connect to database: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to connect to database: {:?}", e);
                }
            }
        }
        Some(Commands::Migration { operation }) => {
            match operation {
                MigrationOperation::Generate { name, db } => {
                    let maybeConn = db::local(db).await;
                    match maybeConn {
                        Err(e) => {
                            println!("Failed to connect to database: {:?}", e);
                        }
                        Ok(conn) => {
                            let introspection_result = db::introspect(&conn).await;
                            match introspection_result {
                                Ok(introspection) => {
                                    let paths = collect_filepaths(&options.in_dir);
                                    match paths.schema_files.as_slice() {
                                        [] => eprintln!("No schema files found!"),
                                        [schema_path] => {
                                            let mut file = fs::File::open(schema_path.clone())?;
                                            let mut schema_str = String::new();
                                            file.read_to_string(&mut schema_str)?;
                                            match parser::run(&schema_str) {
                                                Ok(schema) => {
                                                    let diff =
                                                        diff::diff(&introspection.schema, &schema);
                                                    let sql = migration::to_sql(&diff);

                                                    // Format like {year}{month}{day}{hour}{minute}
                                                    let current_date = chrono::Utc::now()
                                                        .format("%Y%m%d%H%M")
                                                        .to_string();

                                                    // Write the migration file
                                                    let migration_file_path = format!(
                                                        "{}/{}_{}/migration.sql",
                                                        options.migration_dir, current_date, name
                                                    );
                                                    let diff_file_path = format!(
                                                        "{}/{}_{}/schema.diff",
                                                        options.migration_dir, current_date, name
                                                    );

                                                    // Write migration file
                                                    let migration_file =
                                                        Path::new(&migration_file_path);
                                                    let mut output =
                                                        fs::File::create(migration_file);

                                                    match output {
                                                        Ok(mut file) => {
                                                            file.write_all(sql.as_bytes());
                                                        }
                                                        Err(e) => {
                                                            eprintln!(
                                                                "Failed to create file: {:?}",
                                                                e
                                                            );
                                                            return Err(e);
                                                        }
                                                    };

                                                    // Write diff
                                                    let diff_file = Path::new(&diff_file_path);
                                                    let mut output = fs::File::create(diff_file);

                                                    match output {
                                                        Ok(mut file) => {
                                                            let json_diff =
                                                                serde_json::to_string(&diff)
                                                                    .unwrap();
                                                            file.write_all(json_diff.as_bytes());
                                                        }
                                                        Err(e) => {
                                                            eprintln!(
                                                                "Failed to create file: {:?}",
                                                                e
                                                            );
                                                            return Err(e);
                                                        }
                                                    };
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to parse schema: {:?}", e);
                                                }
                                            }
                                        }
                                        _ => eprintln!("Multiple schema files found!"),
                                    }
                                }
                                Err(e) => {
                                    println!("Failed to connect to database: {:?}", e);
                                }
                            }
                        }
                    }
                }
                MigrationOperation::Apply { db } => {
                    let maybeConn = db::local(db).await;
                    match maybeConn {
                        Ok(conn) => {
                            let migration_result = db::migrate(&conn, &options.migration_dir).await;
                            match migration_result {
                                Ok(()) => {
                                    println!("Migration finished!");
                                }
                                Err(e) => {
                                    println!("Failed to connect to database: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Failed to connect to database: {:?}", e);
                        }
                    }
                }
            }
        }
        None => {
            execute(&options, collect_filepaths(&options.in_dir));
        }
    }
    Ok(())
}

fn create_dir_if_not_exists(path: &str) -> io::Result<()> {
    let path = Path::new(path);

    // Check if the path exists and is a directory
    if path.exists() {
        if path.is_dir() {
            // The directory already exists
            Ok(())
        } else {
            // The path exists but is not a directory
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "The path exists but is not a directory",
            ))
        }
    } else {
        // The path does not exist, create the directory
        fs::create_dir_all(path)
    }
}

fn format_all(options: &Options, paths: Found) -> io::Result<()> {
    for schema_file_path in paths.schema_files {
        format_schema(&options, &schema_file_path);
    }

    // Format queries
    for query_file_path in paths.query_files {
        format_query(&options, &query_file_path);
    }

    Ok(())
}

fn format_schema(options: &Options, schema_file_path: &str) -> io::Result<()> {
    let mut schema_file = fs::File::open(schema_file_path)?;
    let mut schema_source_str = String::new();
    schema_file.read_to_string(&mut schema_source_str)?;

    match parser::run(&schema_source_str) {
        Ok(schema) => {
            // Format schema
            let formatted = generate::format::schema_to_string(&schema);
            let path = Path::new(&schema_file_path);
            let mut output = fs::File::create(path).expect("Failed to create file");
            output
                .write_all(formatted.as_bytes())
                .expect("Failed to write to file");
        }
        Err(err) => eprintln!("{:?}", err),
    }

    Ok(())
}

fn write_schema(schema_file_path: &str, schema: &ast::Schema) -> io::Result<()> {
    // Format schema
    let formatted = generate::format::schema_to_string(&schema);
    let path = Path::new(&schema_file_path);
    let mut output = fs::File::create(path);
    match output {
        Ok(mut file) => {
            file.write_all(formatted.as_bytes());
        }
        Err(e) => {
            eprintln!("Failed to create file: {:?}", e);
            return Err(e);
        }
    };

    Ok(())
}

fn format_query(options: &Options, query_file_path: &str) -> io::Result<()> {
    let mut query_file = fs::File::open(query_file_path)?;
    let mut query_source_str = String::new();
    query_file.read_to_string(&mut query_source_str)?;

    match parser::parse_query(&query_source_str) {
        Ok(query_list) => {
            // Format query
            let formatted = generate::format::query(&query_list);
            let path = Path::new(&query_file_path);
            let mut output = fs::File::create(path).expect("Failed to create file");
            output
                .write_all(formatted.as_bytes())
                .expect("Failed to write to file");
        }
        Err(err) => eprintln!("{:?}", err),
    }

    Ok(())
}

fn execute(options: &Options, paths: Found) -> io::Result<()> {
    match paths.schema_files.as_slice() {
        [] => eprintln!("No schema files found!"),
        [schema_path] => {
            let mut file = fs::File::open(schema_path.clone())?;
            let mut input = String::new();
            file.read_to_string(&mut input)?;

            match parser::run(&input) {
                Ok(schema) => {
                    // Generate schema files
                    generate_elm_schema(&options, &schema);
                    generate_typescript_schema(&options, &schema);

                    for query_file_path in paths.query_files {
                        let mut query_file = fs::File::open(query_file_path.clone())?;
                        let mut query_source_str = String::new();
                        query_file.read_to_string(&mut query_source_str)?;

                        match parser::parse_query(&query_source_str) {
                            Ok(query_list) => {
                                // Typecheck and generate
                                let typecheck_result =
                                    typecheck::check_queries(&schema, &query_list);

                                match typecheck_result {
                                    Ok(typecheck_context) => {
                                        create_dir_if_not_exists(&out(&options, "elm/Query"));
                                        create_dir_if_not_exists(&out(
                                            &options,
                                            "typescript/query",
                                        ));
                                        generate::elm::write_queries(
                                            &out(&options, "elm"),
                                            &typecheck_context,
                                            &query_list,
                                        );
                                        generate::typescript::write_queries(
                                            &out(&options, "typescript"),
                                            &typecheck_context,
                                            &query_list,
                                        );
                                    }
                                    Err(err) => eprintln!("{:?}", err),
                                }
                            }
                            Err(err) => eprintln!("{:?}", err),
                        }
                    }
                }

                Err(err) => eprintln!("{:?}", err),
            }
        }

        _ => eprintln!("More than one schema file was found, but for now only one is supported"),
    }

    Ok(())
}

#[derive(Debug)]
struct Found {
    schema_files: Vec<String>,
    query_files: Vec<String>,
}

fn is_schema_file(file_path: &str) -> bool {
    file_path == "schema.pyre" || file_path.ends_with(".schema.pyre")
}

fn collect_filepaths(dir: &str) -> Found {
    let mut schema_files: Vec<String> = vec![];
    let mut query_files: Vec<String> = vec![];

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Convert the path to a string for easier manipulation
        if let Some(file_str) = path.to_str() {
            // Skip files that don't end in `.pyre`
            if !file_str.ends_with(".pyre") {
                continue;
            }

            let path = Path::new(file_str);
            match path.file_name() {
                None => continue,
                Some(os_file_name) => {
                    match os_file_name.to_str() {
                        None => continue,
                        Some(file_name) => {
                            // Check if the file is `schema.pyre`
                            if is_schema_file(file_name) {
                                schema_files.push(file_str.to_string());
                            } else {
                                query_files.push(file_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    Found {
        schema_files,
        query_files,
    }
}
