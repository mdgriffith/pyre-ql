use crate::ext::string;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast;

#[derive(Debug)]
pub struct Error {
    pub error_type: ErrorType,
    pub location: Location,
}

#[derive(Debug)]
pub struct Location {
    pub highlight: Option<Range>,
    pub area: Range,
}

#[derive(Debug)]
pub struct Range {
    pub start: Coord,
    pub end: Coord,
}

#[derive(Debug)]
pub struct Coord {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub enum ErrorType {
    DuplicateDefinition(String),
    DuplicateField {
        record: String,
        field: String,
    },
    DuplicateVariant(String),
    UnknownType(String),

    // Query Validation Errors
    UnknownTable {
        found: String,
        existing: Vec<String>,
    },
    NoFieldsSelected,
    UnknownField {
        found: String,
    },

    LinkToUnknownTable {
        link_name: String,
        unknown_table: String,
    },

    LinkToUnknownField {
        link_name: String,
        unknown_local_field: String,
    },

    LinkToUnknownForeignField {
        link_name: String,
        unknown_foreign_field: String,
    },
}

#[derive(Debug)]
enum DefType {
    Record,
    Tagged,
    Builtin,
}

#[derive(Debug)]
pub struct Context {
    pub types: HashMap<String, DefType>,
    pub tables: HashMap<String, ast::RecordDetails>,
}

pub fn get_linked_table<'a>(
    context: &'a Context,
    link: &'a ast::LinkDetails,
) -> Option<&'a ast::RecordDetails> {
    context
        .tables
        .get(&crate::ext::string::decapitalize(&link.foreign_tablename))
}

fn empty_context() -> Context {
    let mut context = Context {
        types: HashMap::new(),
        tables: HashMap::new(),
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
    let mut errors: Vec<Error> = Vec::new();
    check_schema_definitions(&context, schem, &mut errors);

    Ok(schem)
}

fn populate_context(schem: &ast::Schema, context: &mut Context) -> Vec<Error> {
    let mut errors = Vec::new();

    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, fields } => {
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
                context.tables.insert(
                    crate::ext::string::decapitalize(&name),
                    ast::RecordDetails {
                        name: name.clone(),
                        fields: fields.clone(),
                    },
                );
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

    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, fields } => {
                for field in fields {
                    match field {
                        // ast::Field::Column(ast::Column { name, type_, .. }) => {
                        //     if context.tables[name] {
                        //         errors.push(Error {
                        //             error_type: ErrorType::DuplicateField {
                        //                 record: name.clone(),
                        //                 field: name.clone(),
                        //             },
                        //             location: Location {
                        //                 highlight: None,
                        //                 area: Range {
                        //                     start: Coord { line: 0, column: 0 },
                        //                     end: Coord { line: 0, column: 0 },
                        //                 },
                        //             },
                        //         });
                        //     }
                        // }
                        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                            let maybe_foreign_table = get_linked_table(context, link);

                            match maybe_foreign_table {
                                Some(foreign_table) => {
                                    for foreign_id in &link.foreign_ids {
                                        if !foreign_table
                                            .fields
                                            .iter()
                                            .any(|f| ast::has_fieldname(f, foreign_id))
                                        {
                                            errors.push(Error {
                                                error_type: ErrorType::LinkToUnknownForeignField {
                                                    link_name: link.link_name.clone(),
                                                    unknown_foreign_field: foreign_id.clone(),
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
                                    }
                                }
                                None => {
                                    errors.push(Error {
                                        error_type: ErrorType::LinkToUnknownTable {
                                            link_name: link.link_name.clone(),
                                            unknown_table: link.foreign_tablename.clone(),
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
                            }

                            // Check that the local ids exist
                            for local_id in &link.local_ids {
                                if !fields.iter().any(|f| ast::has_fieldname(f, local_id)) {
                                    errors.push(Error {
                                        error_type: ErrorType::LinkToUnknownField {
                                            link_name: link.link_name.clone(),
                                            unknown_local_field: local_id.clone(),
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
                            }
                        }
                        _ => {}
                    }
                }
            }

            _ => {}
        }
    }

    errors
}

fn check_schema_definitions(context: &Context, schem: &ast::Schema, mut errors: &mut Vec<Error>) {
    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, fields } => {
                let mut field_names = HashSet::new();
                for field in ast::collect_columns(fields) {
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
                                for field in ast::collect_columns(fields) {
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
}

// Check query

pub fn check_queries<'a>(
    schem: &ast::Schema,
    query_list: &ast::QueryList,
) -> Result<Context, Vec<Error>> {
    let mut context = empty_context();
    let population_errors = populate_context(schem, &mut context);
    if !population_errors.is_empty() {
        return Err(population_errors);
    }

    let mut errors: Vec<Error> = Vec::new();
    check_schema_definitions(&context, schem, &mut errors);

    for mut query in &query_list.queries {
        match query {
            ast::QueryDef::Query(q) => check_query(&context, &mut errors, &q),
            _ => continue,
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(context)
}

fn check_query(context: &Context, mut errors: &mut Vec<Error>, query: &ast::Query) {
    // We need to check
    // 1. The field exists on the record in the schema
    //    What type is the field (add to `QueryField`)
    //
    // 2. If params are defined
    //     2.a All defined params are used
    //     2.b Every used param is defined
    //
    for field in &query.fields {
        match context.tables.get(&field.name) {
            None => errors.push(Error {
                error_type: ErrorType::UnknownTable {
                    found: field.name.clone(),
                    existing: vec![],
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            }),
            Some(table) => check_table_query(context, errors, table, field),
        }
    }
}

fn check_table_query(
    context: &Context,
    mut errors: &mut Vec<Error>,
    table: &ast::RecordDetails,
    query: &ast::QueryField,
) {
    if query.fields.is_empty() {
        errors.push(Error {
            error_type: ErrorType::NoFieldsSelected,
            location: Location {
                highlight: None,
                area: Range {
                    start: Coord { line: 0, column: 0 },
                    end: Coord { line: 0, column: 0 },
                },
            },
        })
    }

    // We've already checked that the top-level query field name is valid
    // we want to make sure that every field queried exists in `table` as a column
    for field in ast::collect_query_fields(&query.fields) {
        let mut is_known_field = false;
        for col in &table.fields {
            if is_known_field {
                continue;
            }
            match col {
                ast::Field::Column(column) => {
                    if column.name == field.name {
                        is_known_field = true;
                        check_field(context, errors, column, field)
                    }
                }
                ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                    if link.link_name == field.name {
                        is_known_field = true;
                        check_link(context, errors, link, field)
                    }
                    ()
                }
                _ => (),
            }
        }
        if (!is_known_field) {
            errors.push(Error {
                error_type: ErrorType::UnknownField {
                    found: field.name.clone(),
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            })
        }
    }
}

fn check_field(
    context: &Context,
    mut errors: &mut Vec<Error>,
    column: &ast::Column,
    field: &ast::QueryField,
) {
    if (!field.fields.is_empty()) {
        errors.push(Error {
            error_type: ErrorType::UnknownField {
                found: field.name.clone(),
            },
            location: Location {
                highlight: None,
                area: Range {
                    start: Coord { line: 0, column: 0 },
                    end: Coord { line: 0, column: 0 },
                },
            },
        })
    }
}

fn check_link(
    context: &Context,
    mut errors: &mut Vec<Error>,
    link: &ast::LinkDetails,
    field: &ast::QueryField,
) {
    if (field.fields.is_empty()) {
        errors.push(Error {
            error_type: ErrorType::UnknownField {
                found: field.name.clone(),
            },
            location: Location {
                highlight: None,
                area: Range {
                    start: Coord { line: 0, column: 0 },
                    end: Coord { line: 0, column: 0 },
                },
            },
        })
    } else {
        let table = context
            .tables
            .get(&crate::ext::string::decapitalize(&link.foreign_tablename));
        match table {
            None => errors.push(Error {
                error_type: ErrorType::UnknownTable {
                    found: link.foreign_tablename.clone(),
                    existing: vec![],
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            }),
            Some(table) => check_table_query(context, errors, table, field),
        }
    }
}
