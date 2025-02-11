use chrono;
use colored::*;
use serde_json;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::ast;
use crate::db;
use crate::diff;
use crate::error;
use crate::filesystem;
use crate::format;
use crate::generate;
use crate::parser;
use crate::typecheck;

pub struct Options<'a> {
    pub in_dir: &'a Path,
}

fn id_column() -> ast::Column {
    ast::Column {
        name: "id".to_string(),
        type_: "Int".to_string(),
        serialization_type: ast::SerializationType::Concrete(
            ast::ConcreteSerializationType::Integer,
        ),
        nullable: false,
        directives: vec![ast::ColumnDirective::PrimaryKey],
        start: None,
        end: None,
        start_name: None,
        end_name: None,
        start_typename: None,
        end_typename: None,
    }
}

// Top level commands
pub fn init(options: &Options, multidb: bool) -> io::Result<()> {
    let mut database = ast::Database {
        schemas: Vec::new(),
    };
    let cwd = std::env::current_dir().expect("Failed to get current directory");
    let pyre_dir = cwd.join("pyre");
    if !pyre_dir.exists() {
        fs::create_dir(&pyre_dir).expect("Failed to create pyre directory");
    } else {
        error::format_custom_error(
            "Directory exists",
            "The pyre directory already exists in the current directory",
        );
        std::process::exit(1);
    }

    if multidb {
        let schema_dir = pyre_dir.join("schema");
        filesystem::create_dir_if_not_exists(&schema_dir)?;

        // Create Base Schema
        let base_dir = schema_dir.join("base");
        filesystem::create_dir_if_not_exists(&base_dir)?;

        database.schemas.push(ast::Schema {
            namespace: "Base".to_string(),
            session: None,
            files: vec![ast::SchemaFile {
                path: base_dir.join("schema.pyre").to_string_lossy().to_string(),
                definitions: vec![ast::Definition::Record {
                    name: "User".to_string(),
                    fields: vec![ast::Field::Column(id_column())],
                    start: None,
                    end: None,
                    start_name: None,
                    end_name: None,
                }],
            }],
        });

        // Create User Schema
        let user_dir = schema_dir.join("user");
        filesystem::create_dir_if_not_exists(&user_dir)?;

        database.schemas.push(ast::Schema {
            namespace: "User".to_string(),
            session: None,
            files: vec![ast::SchemaFile {
                path: user_dir.join("schema.pyre").to_string_lossy().to_string(),
                definitions: vec![ast::Definition::Record {
                    name: "Example".to_string(),
                    fields: vec![
                        ast::Field::FieldDirective(ast::FieldDirective::Link(ast::LinkDetails {
                            link_name: "user".to_string(),
                            local_ids: vec!["userId".to_string()],
                            foreign: ast::Qualified {
                                schema: "base".to_string(),
                                table: "User".to_string(),
                                fields: vec!["id".to_string()],
                            },
                            start_name: None,
                            end_name: None,
                        })),
                        ast::Field::Column(id_column()),
                        ast::Field::Column(ast::Column {
                            name: "userId".to_string(),
                            type_: "Int".to_string(),
                            serialization_type: ast::SerializationType::Concrete(
                                ast::ConcreteSerializationType::Integer,
                            ),
                            nullable: false,
                            directives: vec![ast::ColumnDirective::PrimaryKey],
                            start: None,
                            end: None,
                            start_name: None,
                            end_name: None,
                            start_typename: None,
                            end_typename: None,
                        }),
                    ],
                    start: None,
                    end: None,
                    start_name: None,
                    end_name: None,
                }],
            }],
        });
    } else {
        let records = vec![ast::Definition::Record {
            name: "User".to_string(),
            fields: vec![ast::Field::Column(id_column())],
            start: None,
            end: None,
            start_name: None,
            end_name: None,
        }];
        database.schemas.push(ast::Schema {
            namespace: ast::DEFAULT_SCHEMANAME.to_string(),
            session: None,
            files: vec![ast::SchemaFile {
                path: pyre_dir.join("schema.pyre").to_str().unwrap().to_string(),
                definitions: records,
            }],
        });
    }

    format::database(&mut database);
    write_db_schema(options, &database)?;

    Ok(())
}

pub fn generate(options: &Options, out: &str) -> io::Result<()> {
    execute(
        options,
        filesystem::collect_filepaths(&options.in_dir)?,
        Path::new(out),
    )
}

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
                    if filesystem::is_schema_file(file_path) {
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
                    if filesystem::is_schema_file(file_path) {
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

                if filesystem::is_schema_file(&file_path) {
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

pub fn check(options: &Options, files: Vec<String>, json: bool) -> io::Result<()> {
    match run_check(filesystem::collect_filepaths(&options.in_dir)?) {
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
    }
    Ok(())
}

pub async fn introspect<'a>(
    options: &'a Options<'a>,
    database: &str,
    auth: &Option<String>,
    namespace: &Option<String>,
) -> io::Result<()> {
    let conn_result = db::connect(&database.to_string(), auth).await;
    match conn_result {
        Ok(conn) => {
            let full_namespace = namespace
                .clone()
                .unwrap_or(ast::DEFAULT_SCHEMANAME.to_string());
            let introspection_result = db::introspect(&conn, &full_namespace).await;
            match introspection_result {
                Ok(introspection) => {
                    let path: PathBuf = if full_namespace != ast::DEFAULT_SCHEMANAME {
                        Path::new(&options.in_dir)
                            .join("schema")
                            .join(&full_namespace)
                            .join("schema.pyre")
                    } else {
                        Path::new(&options.in_dir).join("schema.pyre")
                    };

                    if path.exists() {
                        println!(
                            "\nSchema already exists\n\n   {}",
                            path.display().to_string().yellow()
                        );
                        println!("\nRemove it if you want to generate a new one!");
                    } else {
                        println!("Schema written to {:?}", path.to_str());

                        if ast::is_empty_schema(&introspection.schema) {
                            println!("I was able to successfully connect to the database, but I couldn't find any tables or views!");
                        } else {
                            write_schema(options, &false, &introspection.schema)?;
                        }
                    }
                }
                Err(libsql_error) => {
                    println!("{}", error::format_libsql_error(&libsql_error));
                }
            }
        }
        Err(err) => {
            println!("{}", err.format_error());
        }
    }
    Ok(())
}

pub async fn migrate<'a>(
    options: &'a Options<'a>,
    database: &str,
    auth: &Option<String>,
    migration_dir: &str,
    namespace: &Option<String>,
) -> io::Result<()> {
    // Namespace is required if there are multiple dbs
    // Otherwise, is disallowed
    check_namespace_requirements(&namespace, &options);
    let namespace_migration_dir = match namespace {
        Some(ns) => Path::new(migration_dir).join(ns),
        None => Path::new(migration_dir).to_path_buf(),
    };

    let connection_result = db::connect(&database.to_string(), auth).await;
    match connection_result {
        Ok(conn) => {
            let migration_result = db::migrate(&conn, &namespace_migration_dir).await;
            match migration_result {
                Ok(()) => {
                    println!("Migration finished!");
                }
                Err(migration_error) => {
                    println!("{}", migration_error.format_error());
                }
            }
        }
        Err(err) => {
            println!("{}", err.format_error());
        }
    }
    Ok(())
}

pub async fn generate_migration<'a>(
    options: &'a Options<'a>,
    name: &str,
    db: &str,
    auth: &Option<String>,
    migration_dir: &Path,
    namespace: &Option<String>,
) -> io::Result<()> {
    // Namespace is required if there are multiple dbs
    // Otherwise, is disallowed
    check_namespace_requirements(&namespace, &options);

    let target_namespace = namespace
        .clone()
        .unwrap_or_else(|| ast::DEFAULT_SCHEMANAME.to_string());

    let target_namespace_dir = match namespace {
        None => migration_dir,
        Some(name) => &migration_dir.join(&name),
    };

    let connection_result = db::connect(&db.to_string(), auth).await;
    match connection_result {
        Err(e) => {
            println!("Failed to connect to database: {:?}", e);
        }
        Ok(conn) => {
            let introspection_result = db::introspect(&conn, &target_namespace).await;
            match introspection_result {
                Ok(introspection) => {
                    let existing_migrations =
                        db::read_migration_items(target_namespace_dir).unwrap_or(vec![]);

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
                    let current_db = parse_database_schemas(&paths)?;

                    match typecheck::check_schema(&current_db) {
                        Ok(context) => {
                            let current_schema = current_db
                                .schemas
                                .iter()
                                .find(|s| s.namespace == target_namespace)
                                .expect("Schema not found");

                            let schema_diff =
                                diff::diff_schema(&introspection.schema, current_schema);

                            filesystem::create_dir_if_not_exists(migration_dir)?;

                            write_migration(
                                &context,
                                current_schema,
                                &schema_diff,
                                name,
                                target_namespace_dir,
                            )?;
                        }
                        Err(error_list) => {
                            for err in error_list {
                                let schema_source =
                                    filesystem::get_schema_source(&err.filepath, &paths)
                                        .unwrap_or("");

                                let formatted_error = error::format_error(&schema_source, &err);

                                eprintln!("{}", &formatted_error);
                            }
                            std::process::exit(1);
                        }
                    }
                }
                Err(err) => {
                    println!("Failed to connect to database: {:?}", err);
                }
            }
        }
    }
    Ok(())
}

//
//   Helpers
//

fn check_namespace_requirements(namespace: &Option<String>, options: &Options) {
    let namespaces_result = filesystem::read_namespaces(Path::new(&options.in_dir));
    match namespaces_result {
        Ok(namespaces_found) => match namespaces_found {
            filesystem::NamespacesFound::Default => {
                if let Some(_) = namespace {
                    println!("{}", error::format_custom_error("Namespace is not needed", "It looks like you only have one schema, which means you don't need to provide a namespace."));
                    std::process::exit(1);
                }
            }
            filesystem::NamespacesFound::MultipleNamespaces(namespaces) => {
                if let Some(namespace) = namespace {
                    if !namespaces.contains(namespace.as_str()) {
                        let error_body = format!(
                            "{} is not one of the allowed namespaces:\n{}",
                            error::yellow_if(true, namespace),
                            error::format_yellow_list(
                                true,
                                namespaces.into_iter().collect::<Vec<_>>()
                            )
                        );
                        let error_message =
                            error::format_custom_error("Unknown Schema", &error_body);
                        println!("{}", error_message);
                        std::process::exit(1);
                    }
                } else {
                    let error_body = format!("It looks like you have multiple schemas:\n{}\n Let me know which one you want to migrate by passing {}",
                            error::format_yellow_list(true, namespaces.into_iter().collect::<Vec<_>>()),
                            error::cyan_if(true, "--namespace SCHEMA_TO_MIGRATE")
                        );
                    let error_message = error::format_custom_error("Unknown Schema", &error_body);
                    println!("{}", error_message);
                    std::process::exit(1);
                }
            }
            filesystem::NamespacesFound::EmptySchemaDir
            | filesystem::NamespacesFound::NothingFound => {
                println!(
                    "{}",
                    error::format_custom_error(
                        "Schema Not Found",
                        "I was trying to find the schema, but it's not available."
                    )
                );
                std::process::exit(1);
            }
        },
        Err(err) => {
            println!("Error reading namespaces: {:?}", err);
            std::process::exit(1);
        }
    }
}

// Generation

fn execute(options: &Options, paths: filesystem::Found, out_dir: &Path) -> io::Result<()> {
    let schema = parse_database_schemas(&paths)?;

    match typecheck::check_schema(&schema) {
        Err(error_list) => {
            // TODO
            for err in error_list {
                let schema_source =
                    filesystem::get_schema_source(&err.filepath, &paths).unwrap_or("");

                let formatted_error = error::format_error(&schema_source, &err);

                eprintln!("{}", &formatted_error);
            }
            std::process::exit(1);
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

// Formatting

fn format_all(options: &Options, paths: filesystem::Found) -> io::Result<()> {
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

// Write schema

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
            println!(
                "{}",
                generate::to_string::schema_to_string(&schema.namespace, schema_file)
            );
            continue;
        }
        let target_filepath = schema_file.path.to_string();
        let mut output = fs::File::create(&target_filepath)?;
        let formatted = generate::to_string::schema_to_string(&schema.namespace, schema_file);
        output.write_all(formatted.as_bytes())?;
    }
    Ok(())
}

fn write_migration(
    context: &typecheck::Context,
    schema: &ast::Schema,

    diff: &diff::SchemaDiff,
    migration_name: &str,
    namespace_folder: &Path,
) -> io::Result<()> {
    let sql = generate::migration::to_sql(context, schema, diff);

    // Format like {year}{month}{day}{hour}{minute}
    let current_date = chrono::Utc::now().format("%Y%m%d%H%M").to_string();

    filesystem::create_dir_if_not_exists(&namespace_folder)?;

    let migration_folder = namespace_folder.join(format!("{}_{}", current_date, migration_name));
    filesystem::create_dir_if_not_exists(&migration_folder)?;

    // Write the migration files
    let migration_file = migration_folder.join("migration.sql");
    let diff_file_path = migration_folder.join("schema.diff");

    // Write migration file
    let output = fs::File::create(migration_file);

    match output {
        Ok(mut file) => {
            file.write_all(sql.as_bytes())?;
        }
        Err(e) => {
            eprintln!("Failed to create file: {:?}", e);
            return Err(e);
        }
    };

    // Write diff
    let diff_file = Path::new(&diff_file_path);
    let output = fs::File::create(diff_file);

    match output {
        Ok(mut file) => {
            let json_diff = serde_json::to_string(diff).unwrap();
            file.write_all(json_diff.as_bytes())?;
        }
        Err(e) => {
            eprintln!("Failed to create file: {:?}", e);
            return Err(e);
        }
    };

    Ok(())
}

// Parsing Schemas

fn parse_single_schema(schema_file_path: &String) -> io::Result<ast::Schema> {
    let mut schema = ast::Schema {
        namespace: ast::DEFAULT_SCHEMANAME.to_string(),
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
        namespace: ast::DEFAULT_SCHEMANAME.to_string(),
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

fn parse_database_schemas(paths: &filesystem::Found) -> io::Result<ast::Database> {
    let mut database = ast::Database {
        schemas: Vec::new(),
    };

    for (namespace, schema_files) in paths.schema_files.iter() {
        let mut schema = ast::Schema {
            namespace: namespace.clone(),
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
    errors: Vec<error::Error>,
}

fn run_check(paths: filesystem::Found) -> io::Result<Vec<FileError>> {
    let schema = parse_database_schemas(&paths)?;
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
