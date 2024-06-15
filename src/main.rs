#![allow(warnings)]
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use generate::migration;

mod ast;
mod diff;
mod ext;
mod generate;
mod parser;
mod typecheck;

fn check_all() -> io::Result<()> {
    let mut file = fs::File::open("examples/schema.pyre")?;
    let mut input = String::new();
    file.read_to_string(&mut input)?;

    match parser::run(&input) {
        Ok(schema) => {
            let mut query_file = fs::File::open("examples/query.pyre")?;
            let mut input_query = String::new();
            query_file.read_to_string(&mut input_query)?;

            match parser::parse_query(&input_query) {
                Ok(query_list) => {
                    let result = typecheck::check_queries(&schema, &query_list);
                    println!("{:?}", result);

                    match result {
                        Ok(typecheck_context) => {
                            println!("Typecheck passed");

                            generate::elm::write_queries(&typecheck_context, &query_list);
                            generate::sql::write_queries(&typecheck_context, &query_list);
                            generate::typescript::write_queries(&typecheck_context, &query_list);
                        }
                        Err(err) => eprintln!("{:?}", err),
                    }
                }
                Err(err) => eprintln!("{:?}", err),
            }
        }

        Err(err) => eprintln!("{:?}", err),
    }
    Ok(())
}

fn generate_elm_schema(schema: &ast::Schema) -> io::Result<()> {
    let formatted_elm = generate::elm::schema(&schema);

    // Top level Elm files
    let elm_file = Path::new("examples/elm/Db.elm");
    let mut output = fs::File::create(elm_file).expect("Failed to create file");
    output
        .write_all(formatted_elm.as_bytes())
        .expect("Failed to write to file");

    // Elm Decoders
    let elm_decoders = generate::elm::to_schema_decoders(&schema);
    let elm_decoder_file = Path::new("examples/elm/Db/Decode.elm");
    let mut output = fs::File::create(elm_decoder_file).expect("Failed to create file");
    output
        .write_all(elm_decoders.as_bytes())
        .expect("Failed to write to file");

    // Elm Encoders
    let elm_encoders = generate::elm::to_schema_encoders(&schema);
    let elm_encoder_file = Path::new("examples/elm/Db/Encode.elm");
    let mut output = fs::File::create(elm_encoder_file).expect("Failed to create file");
    output
        .write_all(elm_encoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

fn generate_typescript_schema(schema: &ast::Schema) -> io::Result<()> {
    let formatted_ts = generate::typescript::schema(&schema);

    // Top level TS files
    let ts_file = Path::new("examples/typescript/db.ts");
    let mut output = fs::File::create(ts_file).expect("Failed to create file");
    output
        .write_all(formatted_ts.as_bytes())
        .expect("Failed to write to file");

    // TS Decoders
    let ts_decoders = generate::typescript::to_schema_decoders(&schema);
    let ts_decoder_file = Path::new("examples/typescript/db/decode.ts");
    let mut output = fs::File::create(ts_decoder_file).expect("Failed to create file");
    output
        .write_all(ts_decoders.as_bytes())
        .expect("Failed to write to file");

    Ok(())
}

fn full_run() -> io::Result<()> {
    // Read the content of the file
    let mut file = fs::File::open("examples/schema.pyre")?;
    let mut input = String::new();
    file.read_to_string(&mut input)?;

    match parser::run(&input) {
        Ok(schema) => {
            // println!("{:?}", schema);
            generate_elm_schema(&schema);
            generate_typescript_schema(&schema);

            // Migration Generation
            let schema_diff = diff::diff(&ast::empty_schema(), &schema);

            let sql = migration::to_sql(&schema_diff);

            let migration_path = Path::new("examples/migration.sql");
            let mut migration_output =
                fs::File::create(migration_path).expect("Failed to create file");
            migration_output
                .write_all(sql.as_bytes())
                .expect("Failed to write to file");
        }
        Err(err) => eprintln!("{:?}", err),
    }

    // Read the content of the file
    let mut query_file = fs::File::open("examples/query.pyre")?;
    let mut input_query = String::new();
    query_file.read_to_string(&mut input_query)?;

    match parser::parse_query(&input_query) {
        Ok(parsed) => {
            println!("{:?}", parsed);
            let formatted = generate::format::query(&parsed);

            let path = Path::new("examples/query_formatted.pyre");
            let mut output = fs::File::create(path).expect("Failed to create file");
            output
                .write_all(formatted.as_bytes())
                .expect("Failed to write to file");

            println!("{}", formatted);
        }
        Err(err) => eprintln!("{:?}", err),
    }

    Ok(())
}

fn main() -> io::Result<()> {
    full_run();
    check_all()
}
