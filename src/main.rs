#![allow(warnings)]
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

mod ast;
mod generate;
mod parser;

fn main() -> io::Result<()> {
    // Read the content of the file
    let mut file = fs::File::open("examples/schema.pyre")?;
    let mut input = String::new();
    file.read_to_string(&mut input)?;

    match parser::run(&input) {
        Ok(parsed) => {
            println!("{:?}", parsed);
            let formatted = generate::format::to_string(&parsed);

            let path = Path::new("examples/formatted.pyre");
            let mut output = fs::File::create(path).expect("Failed to create file");
            output
                .write_all(formatted.as_bytes())
                .expect("Failed to write to file");
        }
        Err(err) => eprintln!("{:?}", err),
    }

    Ok(())
}