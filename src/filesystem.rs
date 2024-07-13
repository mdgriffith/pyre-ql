use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct Schema {
    pub name: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Found {
    pub schema_files: Vec<String>,
    pub query_files: Vec<String>,
}

fn read(schema: &Schema) -> String {
    let mut content = String::new();
    for path in &schema.paths {
        let file_content = std::fs::read_to_string(path).unwrap();
        content.push_str(&file_content);
    }
    content
}

pub fn is_schema_file(file_path: &str) -> bool {
    file_path == "schema.pyre" || file_path.contains("schema")
}

pub fn collect_filepaths(dir: &Path) -> Found {
    let mut schema_files: Vec<String> = vec![];
    let mut query_files: Vec<String> = vec![];

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        let relative_path = path.strip_prefix(dir).unwrap_or(path);

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
                            if is_schema_file(relative_path.to_str().unwrap()) {
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

pub fn create_dir_if_not_exists(path: &Path) -> io::Result<()> {
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
