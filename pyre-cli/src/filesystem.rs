use pyre::ast;
use pyre::filesystem;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use walkdir::WalkDir;

pub fn read_namespaces(dir: &Path) -> io::Result<filesystem::NamespacesFound> {
    // Single schema file
    let schema_pyre_path = dir.join("schema.pyre");
    if schema_pyre_path.exists() {
        return Ok(filesystem::NamespacesFound::Default);
    }

    // Check if there's a schema directory
    let schema_dir = dir.join("schema");
    let mut namespaces: HashSet<String> = HashSet::new();
    let mut has_filepaths = false;

    if schema_dir.exists() && schema_dir.is_dir() {
        for entry in fs::read_dir(&schema_dir)? {
            match entry {
                Ok(entry) => {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        namespaces
                            .insert(entry.file_name().to_str().unwrap_or_default().to_string());
                    } else {
                        has_filepaths = true;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        if namespaces.is_empty() && !has_filepaths {
            return Ok(filesystem::NamespacesFound::EmptySchemaDir);
        } else if namespaces.is_empty() {
            return Ok(filesystem::NamespacesFound::Default);
        } else {
            return Ok(filesystem::NamespacesFound::MultipleNamespaces(namespaces));
        }
    }

    Ok(filesystem::NamespacesFound::NothingFound)
}

// Helper function to get namespace from path
// This function takes a path and a base directory as input and returns the namespace
// of the path relative to the base directory. It assumes the path is in the format
// "/base/schema/{namespace}/file.pyre". If the path is not a subdirectory of
// the base directory, it returns ast::DEFAULT_SCHEMANAME as the namespace.
//
// Examples:
// - "/base/schema/namespace1/file.pyre" -> "namespace1"
// - "/base/schema/namespace2/subdir/file.pyre" -> "namespace2"
// - "/not/base/schema/namespace/file.pyre" -> ast::DEFAULT_SCHEMANAME
pub fn get_namespace(path: &Path, base_dir: &Path) -> String {
    path.strip_prefix(base_dir)
        .ok()
        .and_then(|p| p.components().nth(1))
        .and_then(|c| c.as_os_str().to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| ast::DEFAULT_SCHEMANAME.to_string())
}

pub fn collect_filepaths(dir: &Path) -> io::Result<filesystem::Found> {
    let mut schema_files: HashMap<String, Vec<filesystem::SchemaFile>> = HashMap::new();
    let mut query_files: Vec<String> = vec![];
    let mut namespaces: Vec<String> = vec![];

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // If it's a directory and contains schema.pyre, consider it a namespace
        if path.is_dir() {
            let schema_path = path.join("schema.pyre");
            if schema_path.exists() {
                if let Some(namespace) = path.file_name().and_then(|n| n.to_str()) {
                    namespaces.push(namespace.to_string());
                }
            }
            continue;
        }

        let relative_path = path.strip_prefix(dir).unwrap_or(path);

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
                            if filesystem::is_schema_file(relative_path.to_str().unwrap()) {
                                let mut file = fs::File::open(file_str)?;
                                let mut schema_source = String::new();
                                file.read_to_string(&mut schema_source)?;

                                let schema_file = filesystem::SchemaFile {
                                    path: file_str.to_string(),
                                    content: schema_source,
                                };

                                let namespace = get_namespace(path, dir);
                                schema_files
                                    .entry(namespace)
                                    .or_insert_with(Vec::new)
                                    .push(schema_file);
                            } else {
                                query_files.push(file_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(filesystem::Found {
        schema_files,
        query_files,
        namespaces,
    })
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

/// Writes a collection of generated files to disk under a base directory
pub fn write_generated_files<T: AsRef<[u8]>>(
    base_path: &Path,
    files: Vec<filesystem::GeneratedFile<T>>,
) -> io::Result<()> {
    for file in files {
        let full_path = base_path.join(file.path);

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(full_path, file.contents)?;
    }

    Ok(())
}

// Writing files
