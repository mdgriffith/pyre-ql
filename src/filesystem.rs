use crate::ast;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Found {
    pub schema_files: HashMap<String, Vec<SchemaFile>>,
    pub query_files: Vec<String>,
    pub namespaces: Vec<String>,
}

#[derive(Debug)]
pub struct SchemaFile {
    pub path: String,
    pub content: String,
}

pub fn is_schema_file(file_path: &str) -> bool {
    file_path == "schema.pyre" || file_path.contains("schema")
}

pub fn get_schema_source<'a>(filepath: &'a str, found: &'a Found) -> Option<&'a str> {
    // Search through all namespaces
    for schema_files in found.schema_files.values() {
        for schema_file in schema_files {
            if schema_file.path == filepath {
                return Some(&schema_file.content);
            }
        }
    }
    None
}

#[derive(Debug)]
pub enum NamespacesFound {
    Default,
    MultipleNamespaces(HashSet<String>),
    EmptySchemaDir,
    NothingFound,
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

/// Represents a file to be generated
#[derive(Debug)]
pub struct GeneratedFile<T> {
    pub path: std::path::PathBuf,
    pub contents: T,
}

impl<T> GeneratedFile<T> {
    pub fn new(path: impl Into<PathBuf>, contents: T) -> Self {
        Self {
            path: path.into(),
            contents,
        }
    }
}

// Helper function for common text file generation
pub fn generate_text_file(
    path: impl Into<PathBuf>,
    contents: impl Into<String>,
) -> GeneratedFile<String> {
    GeneratedFile::new(path, contents.into())
}

// Writing files
