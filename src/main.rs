#![allow(warnings)]
use chrono;
use clap::{Parser, Subcommand};
use colored::*;
use generate::migration;
use serde_json;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tokio;
use walkdir::WalkDir;

mod ast;
mod db;
mod diff;
mod error;
mod ext;
mod filesystem;
mod format;
mod generate;
mod hash;
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
    version: bool,

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

        /// Output to stdout instead of files
        #[arg(long, default_value_t = false)]
        to_stdout: bool,
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
    // Generate a migration
    Migration {
        name: String,

        #[arg(long)]
        db: String,
    },
}

fn out(options: &Options, file: &Path) -> PathBuf {
    options.out_dir.join(file)
}

fn out_path(options: &Options, file: &str) -> PathBuf {
    Path::new(&options.out_dir).join(file)
}

fn generate_typescript_schema(options: &Options, schema: &ast::Schema) -> io::Result<()> {
    let formatted_ts = generate::typescript::schema(&schema);

    filesystem::create_dir_if_not_exists(&out_path(options, "typescript"));
    filesystem::create_dir_if_not_exists(&out_path(options, "typescript").join("db"));

    // Top level TS files
    // DB engine as db.ts
    let ts_db_path = out(options, Path::new("typescript/db.ts"));
    let ts_file = Path::new(&ts_db_path);
    let mut output = fs::File::create(ts_file).expect("Failed to create file");
    output
        .write_all(generate::typescript::DB_ENGINE.as_bytes())
        .expect("Failed to write to file");

    // Schema-level data types
    let ts_db_data_path = out(options, Path::new("typescript/db/data.ts"));
    let ts_data_path = Path::new(&ts_db_data_path);
    let mut output_data = fs::File::create(ts_data_path).expect("Failed to create file");
    output_data
        .write_all(formatted_ts.as_bytes())
        .expect("Failed to write to file");

    // TS Decoders
    let ts_db_decoder_path = out(options, Path::new("typescript/db/decode.ts"));
    let ts_decoders = generate::typescript::to_schema_decoders(&schema);
    let ts_decoder_file = Path::new(&ts_db_decoder_path);
    let mut output = fs::File::create(ts_decoder_file).expect("Failed to create file");
    output
        .write_all(ts_decoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

struct Options<'a> {
    in_dir: &'a Path,
    out_dir: &'a Path,
    migration_dir: &'a Path,
}

fn prepare_options<'a>(cli: &'a Cli) -> Options<'a> {
    let mut in_dir = Path::new("pyre");
    match &cli.r#in {
        Some(dir) => in_dir = Path::new(dir),
        None => (),
    }

    let mut out_dir = Path::new("pyre/generated");
    match &cli.out {
        Some(dir) => out_dir = Path::new(dir),
        None => (),
    }

    let mut migration_dir = Path::new("pyre/migrations");
    match &cli.migrations {
        Some(dir) => migration_dir = Path::new(dir),
        None => (),
    }

    Options {
        in_dir: &in_dir,
        out_dir: &out_dir,
        migration_dir: &migration_dir,
    }
}

fn get_stdin() -> io::Result<Option<String>> {
    if atty::is(atty::Stream::Stdin) {
        // The above seems backwards to me
        // But this is what the docs say: https://github.com/softprops/atty
        Ok(None)
    } else {
        // Read from stdin
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        Ok(Some(input))
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if let true = cli.version {
        println!("0.1.0");
        return Ok(());
    }

    let options = prepare_options(&cli);

    match &cli.command {
        Some(Commands::Format { files, to_stdout }) => match files.len() {
            0 => match get_stdin()? {
                Some(stdin) => {
                    let paths = filesystem::collect_filepaths(&options.in_dir)?;
                    let mut schema = parse_schemas(&options, &paths)?;

                    // We're assuming this file is a query because we don't have a filepath
                    format_query_to_std_out(&options, &schema, &stdin);
                }
                None => {
                    println!("Formatting all files in {}", options.in_dir.display());
                    format_all(&options, filesystem::collect_filepaths(&options.in_dir)?);
                }
            },
            1 => {
                let file_path = &files[0];

                match get_stdin()? {
                    Some(stdin) => {
                        if filesystem::is_schema_file(file_path) {
                            let mut schema = parse_single_schema_from_source(file_path, &stdin)?;
                            format::schema(&mut schema);
                            // Always write to stdout if stdin is provided
                            write_schema(&options, &true, &schema);
                        } else {
                            let paths = filesystem::collect_filepaths(&options.in_dir)?;
                            let mut schema = parse_schemas(&options, &paths)?;

                            format_query_to_std_out(&options, &schema, &stdin);
                        }
                    }
                    None => {
                        if filesystem::is_schema_file(file_path) {
                            let mut schema = parse_single_schema(file_path)?;
                            format::schema(&mut schema);
                            write_schema(&options, to_stdout, &schema);
                        } else {
                            let paths = filesystem::collect_filepaths(&options.in_dir)?;
                            let mut schema = parse_schemas(&options, &paths)?;

                            format_query(&options, &schema, to_stdout, file_path);
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

                    if filesystem::is_schema_file(file_path) {
                        let mut schema = parse_single_schema(file_path)?;
                        format::schema(&mut schema);
                        write_schema(&options, to_stdout, &schema);
                    } else {
                        let paths = filesystem::collect_filepaths(&options.in_dir)?;
                        let mut schema = parse_schemas(&options, &paths)?;

                        format_query(&options, &schema, to_stdout, file_path);
                    }
                }
                if !to_stdout {
                    println!("{} files were formatted", files.len());
                }
            }
        },
        Some(Commands::Introspect { db }) => {
            let maybeConn = db::local(db).await;
            match maybeConn {
                Ok(conn) => {
                    let introspection_result = db::introspect(&conn).await;
                    match introspection_result {
                        Ok(mut introspection) => {
                            let path: PathBuf = Path::new(&options.in_dir).join("schema.pyre");

                            if path.exists() {
                                println!(
                                    "Schema already exists\n\n   {}",
                                    path.display().to_string().yellow()
                                );

                                println!("\nRemove it if you want to generate a new one!");
                            } else {
                                println!("Schema written to {:?}", path.to_str());
                                write_schema(&options, &false, &introspection.schema);
                            }
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
        Some(Commands::Migration { name, db }) => {
            let maybeConn = db::local(db).await;
            match maybeConn {
                Err(e) => {
                    println!("Failed to connect to database: {:?}", e);
                }
                Ok(conn) => {
                    let introspection_result = db::introspect(&conn).await;
                    match introspection_result {
                        Ok(introspection) => {
                            let migration_dir = Path::new(&options.migration_dir);
                            let existing_migrations =
                                db::read_migration_items(migration_dir).unwrap_or(vec![]);

                            let mut not_applied: Vec<String> = vec![];
                            for migration_from_file in existing_migrations {
                                let mut migrated = false;
                                for migration_recorded in introspection.migrations_recorded.iter() {
                                    if &migration_from_file == migration_recorded {
                                        migrated = true;
                                        break;
                                    }
                                }
                                if !migrated {
                                    not_applied.push(migration_from_file.yellow().to_string());
                                }
                            }
                            if not_applied.len() > 0 {
                                println!(
                                                    "\nIt looks like some migrations have not been applied:\n\n    {}",
                                                    not_applied.join("\n   ")
                                                );
                                println!(
                                                    "\nRun `pyre migrate` to apply these migrations before generating a new one.",
                                                );
                                return Ok(());
                            }

                            // filepaths to .pyre files
                            let paths = filesystem::collect_filepaths(&options.in_dir)?;
                            let schema = parse_schemas(&options, &paths)?;

                            let diff = diff::diff(&introspection.schema, &schema);
                            let sql = migration::to_sql(&schema, &diff);

                            // Format like {year}{month}{day}{hour}{minute}
                            let current_date = chrono::Utc::now().format("%Y%m%d%H%M").to_string();

                            filesystem::create_dir_if_not_exists(&migration_dir);

                            let new_folder = format!("{}_{}", current_date, name);

                            let full_path: PathBuf = migration_dir.join(new_folder);

                            filesystem::create_dir_if_not_exists(&full_path);

                            // Write the migration file
                            let migration_file = options
                                .migration_dir
                                .join(format!("{}_{}/migration.sql", current_date, name));

                            let diff_file_path = options
                                .migration_dir
                                .join(format!("{}_{}/schema.diff", current_date, name));

                            // Write migration file
                            let mut output = fs::File::create(migration_file);

                            match output {
                                Ok(mut file) => {
                                    file.write_all(sql.as_bytes());
                                }
                                Err(e) => {
                                    eprintln!("Failed to create file: {:?}", e);
                                    return Err(e);
                                }
                            };

                            // Write diff
                            let diff_file = Path::new(&diff_file_path);
                            let mut output = fs::File::create(diff_file);

                            match output {
                                Ok(mut file) => {
                                    let json_diff = serde_json::to_string(&diff).unwrap();
                                    file.write_all(json_diff.as_bytes());
                                }
                                Err(e) => {
                                    eprintln!("Failed to create file: {:?}", e);
                                    return Err(e);
                                }
                            };
                        }
                        Err(err) => {
                            println!("Failed to connect to database: {:?}", err);
                        }
                    }
                }
            }
        }
        None => {
            execute(&options, filesystem::collect_filepaths(&options.in_dir)?);
        }
    }
    Ok(())
}

fn format_all(options: &Options, paths: filesystem::Found) -> io::Result<()> {
    let mut schema = parse_schemas(&options, &paths)?;

    format::schema(&mut schema);
    write_schema(options, &false, &schema);

    // Format queries
    for query_file_path in paths.query_files {
        format_query(&options, &schema, &false, &query_file_path);
    }

    Ok(())
}

fn write_schema(options: &Options, to_stdout: &bool, schema: &ast::Schema) -> io::Result<()> {
    // Format schema

    for schema_file in schema.files.iter() {
        if *to_stdout {
            println!("{}", generate::to_string::schema_to_string(&schema_file));
            continue;
        }
        let mut output = fs::File::create(options.out_dir.join(schema_file.path.to_string()));
        let formatted = generate::to_string::schema_to_string(&schema_file);
        match output {
            Ok(mut file) => {
                file.write_all(formatted.as_bytes());
            }
            Err(e) => {
                eprintln!("Failed to create file: {:?}", e);
                return Err(e);
            }
        };
    }

    Ok(())
}

fn format_query_to_std_out(
    options: &Options,
    schema: &ast::Schema,
    query_source_str: &str,
) -> io::Result<()> {
    match parser::parse_query("stdin", query_source_str) {
        Ok(mut query_list) => {
            // Format query
            format::query_list(schema, &mut query_list);

            // Convert to string
            let formatted = generate::to_string::query(&query_list);

            println!("{}", formatted);
            return Ok(());
        }
        Err(e) => {
            // println!("Failed to parse query: {:#?}", e);
            println!("{}", query_source_str);
            return Ok(());
        }
    }
    Ok(())
}

fn format_query(
    options: &Options,
    schema: &ast::Schema,
    to_stdout: &bool,
    query_file_path: &str,
) -> io::Result<()> {
    let mut query_file = fs::File::open(query_file_path)?;
    let mut query_source_str = String::new();
    query_file.read_to_string(&mut query_source_str)?;

    match parser::parse_query(query_file_path, &query_source_str) {
        Ok(mut query_list) => {
            // Format query
            format::query_list(schema, &mut query_list);

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

fn parse_single_schema(schema_file_path: &String) -> io::Result<ast::Schema> {
    let mut schema = ast::Schema { files: vec![] };

    let mut file = fs::File::open(schema_file_path.clone())?;
    let mut schema_source = String::new();
    file.read_to_string(&mut schema_source)?;

    match parser::run(&schema_file_path, &schema_source, &mut schema) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{}", parser::render_error(&schema_source, err));
        }
    }

    Ok(schema)
}

fn parse_single_schema_from_source(
    schema_file_path: &str,
    schema_source: &str,
) -> io::Result<ast::Schema> {
    let mut schema = ast::Schema { files: vec![] };

    match parser::run(&schema_file_path, &schema_source, &mut schema) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{}", parser::render_error(&schema_source, err));
        }
    }

    Ok(schema)
}

fn parse_schemas(options: &Options, paths: &filesystem::Found) -> io::Result<ast::Schema> {
    let mut schema = ast::Schema { files: vec![] };
    for source in paths.schema_files.iter() {
        match parser::run(&source.path, &source.content, &mut schema) {
            Ok(()) => {}
            Err(err) => {
                eprintln!("{}", parser::render_error(&source.content, err));
            }
        }
    }

    Ok(schema)
}

fn execute(options: &Options, paths: filesystem::Found) -> io::Result<()> {
    let schema = parse_schemas(&options, &paths)?;

    match typecheck::check_schema(&schema) {
        Err(errorList) => {
            // TODO
            for err in errorList {
                let schema_source =
                    filesystem::get_schema_source(&err.filepath, &paths).unwrap_or("");

                let formatted_error = error::format_error(&schema_source, &err);

                print!("{}", &formatted_error);
            }
        }
        Ok(mut context) => {
            // Generate schema files
            generate::elm::write(&options.out_dir, &schema);
            generate_typescript_schema(&options, &schema);

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
                            Ok(()) => {
                                filesystem::create_dir_if_not_exists(
                                    &out_path(&options, "elm").join("Query"),
                                );
                                filesystem::create_dir_if_not_exists(
                                    &out_path(&options, "typescript").join("query"),
                                );
                                generate::elm::write_queries(
                                    &out(&options, Path::new("elm")),
                                    &context,
                                    &query_list,
                                );
                                generate::typescript::write_queries(
                                    &out(&options, Path::new("typescript")),
                                    &context,
                                    &query_list,
                                );
                            }
                            Err(errorList) => {
                                let mut errors = "".to_string();
                                for err in errorList {
                                    let formatted_error =
                                        error::format_error(&query_source_str, &err);
                                    errors.push_str(&formatted_error);
                                }

                                println!("{}", errors);
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("{}", parser::render_error(&query_source_str, err))
                    }
                }
            }
        }
    }

    Ok(())
}
