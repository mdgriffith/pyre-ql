use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast;

pub struct Error {
    pub error_type: ErrorType,
    pub location: Location,
}

pub struct Location {
    pub highlight: Option<Range>,
    pub area: Range,
}

pub struct Range {
    pub start: Coord,
    pub end: Coord,
}

pub struct Coord {
    pub line: usize,
    pub column: usize,
}

pub enum ErrorType {
    DuplicateDefinition(String),
    DuplicateField { record: String, field: String },
    DuplicateVariant(String),
    UnknownType(String),
}

enum DefType {
    Record,
    Tagged,
    Builtin,
}

pub struct Context {
    types: HashMap<String, DefType>,
}

fn empty_context() -> Context {
    let mut context = Context {
        types: HashMap::new(),
    };
    context.types.insert("String".to_string(), DefType::Builtin);
    context.types.insert("Int".to_string(), DefType::Builtin);
    context.types.insert("Float".to_string(), DefType::Builtin);
    context.types.insert("Bool".to_string(), DefType::Builtin);

    context
}

pub fn check_schema(schem: &ast::Schema) -> Result<&ast::Schema, Vec<Error>> {
    let mut context = empty_context();
    let population_errors = populate_context(schem, &mut context);
    if !population_errors.is_empty() {
        return Err(population_errors);
    }

    check_schema_definitions(context, schem);

    Ok(schem)
}

fn populate_context(schem: &ast::Schema, context: &mut Context) -> Vec<Error> {
    let mut errors = Vec::new();

    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, .. } => {
                if context.types.contains_key(name) {
                    errors.push(Error {
                        error_type: ErrorType::DuplicateDefinition(name.clone()),
                        location: Location {
                            highlight: None,
                            area: Range {
                                start: Coord { line: 0, column: 0 },
                                end: Coord { line: 0, column: 0 },
                            },
                        },
                    });
                }
                context.types.insert(name.clone(), DefType::Record);
            }
            ast::Definition::Tagged { name, .. } => {
                if context.types.contains_key(name) {
                    errors.push(Error {
                        error_type: ErrorType::DuplicateDefinition(name.clone()),
                        location: Location {
                            highlight: None,
                            area: Range {
                                start: Coord { line: 0, column: 0 },
                                end: Coord { line: 0, column: 0 },
                            },
                        },
                    });
                }
                context.types.insert(name.clone(), DefType::Tagged);
            }
            _ => {}
        }
    }

    errors
}

fn check_schema_definitions(context: Context, schem: &ast::Schema) -> Vec<Error> {
    let mut errors = Vec::new();

    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, fields } => {
                let mut field_names = HashSet::new();
                for field in fields {
                    // Type exists check
                    if !context.types.contains_key(&field.type_) {
                        errors.push(Error {
                            error_type: ErrorType::UnknownType(field.type_.clone()),
                            location: Location {
                                highlight: None,
                                area: Range {
                                    start: Coord { line: 0, column: 0 },
                                    end: Coord { line: 0, column: 0 },
                                },
                            },
                        });
                    }

                    // Duplicate field check
                    if field_names.contains(&field.name) {
                        errors.push(Error {
                            error_type: ErrorType::DuplicateField {
                                record: name.clone(),
                                field: field.name.clone(),
                            },
                            location: Location {
                                highlight: None,
                                area: Range {
                                    start: Coord { line: 0, column: 0 },
                                    end: Coord { line: 0, column: 0 },
                                },
                            },
                        });
                    }
                    field_names.insert(field.name.clone());
                }
            }
            ast::Definition::Tagged { name, variants } => {
                for variant in variants {
                    match variant {
                        ast::Variant { name, data } => {
                            if context.types.contains_key(name) {
                                errors.push(Error {
                                    error_type: ErrorType::DuplicateVariant(name.clone()),
                                    location: Location {
                                        highlight: None,
                                        area: Range {
                                            start: Coord { line: 0, column: 0 },
                                            end: Coord { line: 0, column: 0 },
                                        },
                                    },
                                });
                            }
                            if let Some(fields) = data {
                                for field in fields {
                                    if !context.types.contains_key(&field.type_) {
                                        errors.push(Error {
                                            error_type: ErrorType::UnknownType(field.type_.clone()),
                                            location: Location {
                                                highlight: None,
                                                area: Range {
                                                    start: Coord { line: 0, column: 0 },
                                                    end: Coord { line: 0, column: 0 },
                                                },
                                            },
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    errors
}

// Check query

pub fn check_queries(query_list: &ast::QueryList) -> Result<&ast::QueryList, Error> {
    Ok(query_list)
}
