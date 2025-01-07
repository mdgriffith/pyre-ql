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
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The input directory to read from.
    #[arg(long, global = true, default_value = "pyre")]
    r#in: String,

    #[arg(long, global = true)]
    version: bool,

}

#[derive(Subcommand)]
enum Commands {

    /// Generate files for querying your pyre schema.
    Generate {

        /// Directory where output files will be written.
        #[arg(long, default_value = "pyre/generated")]
        out: String,
    },
    /// Format files
    Format {

        #[arg(required = false)]
        files: Vec<String>,

        /// Output to stdout instead of files
        #[arg(long, default_value_t = false)]
        to_stdout: bool,
    },

    /// Typecheck your schema and queries.
    Check {
        #[arg(required = false)]
        files: Vec<String>,

        /// Format errors as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Introspect a database and generate a pyre schema.
    Introspect {

        /// A local filename, or a url, or an environment variable if prefixed with a $.
        database: String,

        ///
        #[arg(long)]
        namespace: Option<String>,

        #[arg(long)]
        auth: Option<String>,
    },

    /// Execute any migrations that are needed.
    Migrate {
        /// A local filename, or a url, or an environment variable if prefixed with a $.
        database: String,

        #[arg(long)]
        auth: Option<String>,

        /// Directory where migration files are stored.
        #[arg(long, default_value = "pyre/migrations")]
        migration_dir: String,
    },

    /// Generate a migration
    Migration {
        /// The migration name.
        name: String,

        #[arg(long)]
        db: String,

        #[arg(long)]
        auth: Option<String>,

        /// Directory where migration files are stored.
        #[arg(long, default_value = "pyre/migrations")]
        migration_dir: String,
    },
}


fn generate_typescript_schema(options: &Options, database: &ast::Database, out_dir: &Path) -> io::Result<()> {
    filesystem::create_dir_if_not_exists(&out_dir.join("typescript"));
    filesystem::create_dir_if_not_exists(&out_dir.join("typescript").join("db"));

    // Top level TS files
    // DB engine as db.ts
    let ts_db_path = &out_dir.join(Path::new("typescript/db.ts"));
    let ts_file = Path::new(&ts_db_path);
    let mut output = fs::File::create(ts_file).expect("Failed to create file");
    output
        .write_all(generate::typescript::DB_ENGINE.as_bytes())
        .expect("Failed to write to file");

    // Session types as db/session.ts
    let ts_session_path_str = &out_dir.join(Path::new("typescript/db/session.ts"));
    let ts_session_path = Path::new(&ts_session_path_str);
    let mut output = fs::File::create(ts_session_path).expect("Failed to create file");
    if let Some(session_ts) = generate::typescript::session(&database) {
        output
            .write_all(session_ts.as_bytes())
            .expect("Failed to write to file");
    }

    // Schema-level data types
    let ts_db_data_path = &out_dir.join(Path::new("typescript/db/data.ts"));
    let ts_data_path = Path::new(&ts_db_data_path);
    let mut output_data = fs::File::create(ts_data_path).expect("Failed to create file");
    let formatted_ts = generate::typescript::schema(&database);
    output_data
        .write_all(formatted_ts.as_bytes())
        .expect("Failed to write to file");

    // TS Decoders
    let ts_db_decoder_path = &out_dir.join(Path::new("typescript/db/decode.ts"));
    let ts_decoders = generate::typescript::to_schema_decoders(&database);
    let ts_decoder_file = Path::new(&ts_db_decoder_path);
    let mut output = fs::File::create(ts_decoder_file).expect("Failed to create file");
    output
        .write_all(ts_decoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

struct Options<'a> {
    in_dir: &'a Path,
}

fn prepare_options<'a>(cli: &'a Cli) -> Options<'a> {
    Options {
        in_dir: Path::new(&cli.r#in),
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
        Commands::Generate { out } => {
            execute(&options, filesystem::collect_filepaths(&options.in_dir)?, Path::new(out));
        },
        Commands::Format { files, to_stdout } => match files.len() {
            0 => match get_stdin()? {
                Some(stdin) => {
                    let paths = filesystem::collect_filepaths(&options.in_dir)?;
                    let mut schema = parse_database_schemas(&options, &paths)?;

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
                            let mut schema = parse_database_schemas(&options, &paths)?;

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
                            let mut database = parse_database_schemas(&options, &paths)?;

                            format_query(&options, &database, to_stdout, file_path);
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
                        let mut database = parse_database_schemas(&options, &paths)?;

                        format_query(&options, &database, to_stdout, file_path);
                    }
                }
                if !to_stdout {
                    println!("{} files were formatted", files.len());
                }
            }
        },
        Commands::Check { files, json } => {
            match check(&options, filesystem::collect_filepaths(&options.in_dir)?) {
                Ok(errors) => {
                    let has_errors = !errors.is_empty();
                    if *json {
                        let mut formatted_errors = Vec::new();
                        for file_error in errors {
                            for error in &file_error.errors {
                                formatted_errors.push(error::format_json(error));
                            }
                        }
                        println!("{}", serde_json::to_string_pretty(&formatted_errors).unwrap());
                        // eprintln!("{}", &formatted_error);
                    } else {
                        for file_error in errors {
                            for err in &file_error.errors {
                                let formatted_error = error::format_error(&file_error.source, err);
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
            };
        }
        Commands::Introspect { database, auth , namespace} => {
            let maybeConn = db::connect(database, auth).await;
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

                                if (ast::is_empty_db(&introspection.schema)) {
                                    println!("I was able to successfully connect to the database, but I couldn't find any tables or views!");
                                } else {
                                    write_db_schema(&options,  &introspection.schema);
                                }
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
        Commands::Migrate { database, auth, migration_dir } => {
            let connection_result = db::connect(database, auth).await;
            match connection_result {
                Ok(conn) => {
                    let migration_result = db::migrate(&conn, Path::new(&migration_dir)).await;
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
        Commands::Migration { name, db, auth, migration_dir } => {
            let connection_result = db::connect(db, auth).await;
            match connection_result {
                Err(e) => {
                    println!("Failed to connect to database: {:?}", e);
                }
                Ok(conn) => {
                    let introspection_result = db::introspect(&conn).await;
                    match introspection_result {
                        Ok(introspection) => {
                            let migration_dir = Path::new(migration_dir);
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
                                println!("\nRun `pyre migrate` to apply these migrations before generating a new one.",);
                                return Ok(());
                            }

                            // filepaths to .pyre files
                            let paths = filesystem::collect_filepaths(&options.in_dir)?;
                            let current_db = parse_database_schemas(&options, &paths)?;


                            let diff = diff::diff(&introspection.schema, &current_db);

                            for (namespace, (schema_diff)) in diff.iter() {
                                if let Some(schema) = ast::get_schema_by_name(&current_db, &namespace) {
                                    write_migration(schema, schema_diff, migration_dir, namespace);
                                }
                            }

                        }
                        Err(err) => {
                            println!("Failed to connect to database: {:?}", err);
                        }
                    }
                }
            }
        }

    }
    Ok(())
}



fn write_migration(
    schema: &ast::Schema,
    diff: &diff::SchemaDiff,
    base_migration_folder: &Path,
    namespace: &String,
) -> io::Result<()> {
    let sql = migration::to_sql(schema, diff);

    // Format like {year}{month}{day}{hour}{minute}
    let current_date = chrono::Utc::now().format("%Y%m%d%H%M").to_string();

    filesystem::create_dir_if_not_exists(base_migration_folder);

    // Write the namespace folder
    let namespace_folder = base_migration_folder.join(namespace.clone());
    filesystem::create_dir_if_not_exists(&namespace_folder);


    // Write the migration files
    let migration_file = namespace_folder.join(format!("{}_migration.sql", current_date));
    let diff_file_path = namespace_folder.join(format!("{}_schema.diff", current_date));

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
            let json_diff = serde_json::to_string(diff).unwrap();
            file.write_all(json_diff.as_bytes());
        }
        Err(e) => {
            eprintln!("Failed to create file: {:?}", e);
            return Err(e);
        }
    };

    Ok(())
}




fn format_all(options: &Options, paths: filesystem::Found) -> io::Result<()> {
    let mut database = parse_database_schemas(&options, &paths)?;

    format::database(&mut database);
    write_db_schema(options, &database);

    // Format queries
    for query_file_path in paths.query_files {
        format_query(&options, &database, &false, &query_file_path);
    }

    Ok(())
}


fn write_db_schema(options: &Options, database: &ast::Database) -> io::Result<()> {
    for schema in database.schemas.iter() {
        write_schema(options, &false, &schema)?;
    }
    Ok(())
}


fn write_schema(options: &Options, to_stdout: &bool, schema: &ast::Schema) -> io::Result<()> {
    // Format schema
    for schema_file in &schema.files {
        if *to_stdout {
            println!("{}", generate::to_string::schema_to_string(schema_file));
            continue;
        }
        let target_filepath = schema_file.path.to_string();
        let mut output = fs::File::create(&target_filepath)?;
        let formatted = generate::to_string::schema_to_string(schema_file);
        output.write_all(formatted.as_bytes())?;
    }
    Ok(())
}


fn format_query_to_std_out(
    options: &Options,
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
        Err(e) => {
            println!("{}", query_source_str);
            return Ok(());
        }
    }
}



fn format_query(
    options: &Options,
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

fn parse_single_schema(schema_file_path: &String) -> io::Result<ast::Schema> {
    let mut schema = ast::Schema {
        namespace: None,
        files: vec![],
        session: None,
    };

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
    let mut schema = ast::Schema {
        namespace: None,
        session: None,
        files: vec![],
    };

    match parser::run(&schema_file_path, &schema_source, &mut schema) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{}", parser::render_error(&schema_source, err));
        }
    }

    Ok(schema)
}

fn parse_database_schemas(options: &Options, paths: &filesystem::Found) -> io::Result<ast::Database> {
    let mut database = ast::Database {
        schemas: Vec::new(),
    };

    for (namespace, schema_files) in paths.schema_files.iter() {
        let mut schema = ast::Schema {
            namespace: Some(namespace.clone()),
            session: None,
            files: vec![],
        };

        for source in schema_files.iter() {
            match parser::run(&source.path, &source.content, &mut schema) {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("{}", parser::render_error(&source.content, err));
                    std::process::exit(1);
                }
            }
        }

        database.schemas.push(schema);
    }

    Ok(database)
}


#[derive(Debug)]
struct FileError {
    source: String,
    errors: Vec<error::Error>
}


fn check(options: &Options, paths: filesystem::Found) -> io::Result<Vec<FileError>> {
    let schema = parse_database_schemas(&options, &paths)?;
    let mut all_file_errors = Vec::new();

    match typecheck::check_schema(&schema) {
        Err(errors) => {
            // Schema errors get grouped under "schema.pyre" or similar
            all_file_errors.push(FileError {
                source: "schema.pyre".to_string(),
                errors: errors,
            });
        }
        Ok(mut context) => {
            for query_file_path in paths.query_files {
                let mut file_errors = Vec::new();
                let mut query_file = fs::File::open(query_file_path.clone())?;
                let mut query_source_str = String::new();
                query_file.read_to_string(&mut query_source_str)?;

                match parser::parse_query(&query_file_path, &query_source_str) {
                    Ok(query_list) => {
                        context.current_filepath = query_file_path.clone();
                        let typecheck_result =
                            typecheck::check_queries(&schema, &query_list, &mut context);

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

fn execute(options: &Options, paths: filesystem::Found, out_dir: &Path) -> io::Result<()> {
    let schema = parse_database_schemas(&options, &paths)?;

    match typecheck::check_schema(&schema) {
        Err(errorList) => {
            // TODO
            for err in errorList {
                let schema_source =
                    filesystem::get_schema_source(&err.filepath, &paths).unwrap_or("");

                let formatted_error = error::format_error(&schema_source, &err);

                eprintln!("{}", &formatted_error);
            }
            std::process::exit(1);
        }
        Ok(mut context) => {
            // Generate schema files
            generate::elm::write(out_dir, &schema);
            generate_typescript_schema(&options, &schema, out_dir);

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
                            Ok(query_params) => {
                                filesystem::create_dir_if_not_exists(
                                    &out_dir.join( "elm").join("Query"),
                                );
                                filesystem::create_dir_if_not_exists(
                                    &out_dir.join( "typescript").join("query"),
                                );
                                generate::elm::write_queries(
                                    &out_dir.join("elm"),
                                    &context,
                                    &query_list,
                                );
                                generate::typescript::write_queries(
                                    &out_dir.join("typescript"),
                                    &context,
                                    &query_params,
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
