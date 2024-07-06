use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct Found {
    pub schema_files: Vec<String>,
    pub query_files: Vec<String>,
}

pub fn is_schema_file(file_path: &str) -> bool {
    file_path == "schema.pyre" || file_path.contains("schema")
}

pub fn collect_filepaths(dir: &Path) -> Found {
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
