use crate::ext::string;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast;

#[derive(Debug)]
pub struct Error {
    pub error_type: ErrorType,
    pub location: Location,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub highlight: Option<Range>,
    pub area: Range,
}

#[derive(Debug, Clone)]
pub struct Range {
    pub start: Coord,
    pub end: Coord,
}

#[derive(Debug, Clone)]
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
    DuplicateVariant {
        base_variant: VariantDef,
        duplicates: Vec<VariantDef>,
    },
    UnknownType(String),
    NoPrimaryKey {
        record: String,
    },
    MultiplePrimaryKeys {
        record: String,
        field: String,
    },
    MultipleTableNames {
        record: String,
        tablenames: Vec<String>,
    },
    // Schema Link errors
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

    // Query Validation Errors
    UnknownTable {
        found: String,
        existing: Vec<String>,
    },
    DuplicateQueryField {
        query: String,
        field: String,
    },
    NoFieldsSelected,
    UnknownField {
        found: String,
    },
    MultipleLimits {
        query: String,
    },
    MultipleOffsets {
        query: String,
    },
    MultipleWheres {
        query: String,
    },
    UndeclaredVariable {
        variable: String,
    },
    WhereOnLinkIsntAllowed {
        link_name: String,
    },
    TypeMismatch {
        table: String,
        column_defined_as: String,
        variable_name: String,
        variable_defined_as: String,
    },
    UnusedParam {
        param: String,
    },
    UndefinedParam {
        param: String,
        type_: String,
    },
    NoSetsInSelect {
        field: String,
    },
    NoSetsInDelete {
        field: String,
    },
    MissingSetInInsert {
        field: String,
    },
    LinksDisallowedInInserts {
        field: String,
    },
    LinksDisallowedInDeletes {
        field: String,
    },
    LinksDisallowedInUpdates {
        field: String,
    },
    InsertColumnIsNotSet {
        field: String,
    },
    InsertMissingColumn {
        field: String,
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
    pub variants: HashMap<String, Vec<VariantDef>>,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub typename: String,
    pub variant_name: String,
    pub location: Location,
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
        variants: HashMap::new(),
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
            ast::Definition::Tagged { name, variants, .. } => {
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

                for mut variant in variants {
                    let location = Location {
                        highlight: None,
                        area: Range {
                            start: Coord { line: 0, column: 0 },
                            end: Coord { line: 0, column: 0 },
                        },
                    };
                    let variant_def = VariantDef {
                        typename: name.clone(),
                        variant_name: variant.name.clone(),
                        location,
                    };

                    context
                        .variants
                        .entry(variant.name.clone())
                        .or_insert_with(Vec::new)
                        .push(variant_def);
                }
            }
            _ => {}
        }
    }

    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record { name, fields } => {
                let mut tablenames: Vec<String> = Vec::new();
                let mut has_primary_id = false;
                let mut field_names = HashSet::new();

                for field in fields {
                    match field {
                        ast::Field::Column(ast::Column {
                            name,
                            type_,
                            directives,
                            ..
                        }) => {
                            if field_names.contains(name) {
                                errors.push(Error {
                                    error_type: ErrorType::DuplicateField {
                                        record: name.clone(),
                                        field: name.clone(),
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
                            if (directives
                                .iter()
                                .any(|item| *item == ast::ColumnDirective::PrimaryKey))
                            {
                                if has_primary_id {
                                    errors.push(Error {
                                        error_type: ErrorType::MultiplePrimaryKeys {
                                            record: name.clone(),
                                            field: name.clone(),
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
                                has_primary_id = true;
                            }

                            field_names.insert(name.clone());
                        }
                        ast::Field::FieldDirective(ast::FieldDirective::TableName(tablename)) => {
                            tablenames.push(tablename.to_string())
                        }
                        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                            let maybe_foreign_table = get_linked_table(context, link);

                            if field_names.contains(&link.link_name) {
                                errors.push(Error {
                                    error_type: ErrorType::DuplicateField {
                                        record: name.clone(),
                                        field: link.link_name.clone(),
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

                if tablenames.len() > 1 {
                    errors.push(Error {
                        error_type: ErrorType::MultipleTableNames {
                            record: name.clone(),
                            tablenames: tablenames.clone(),
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

                if !has_primary_id {
                    errors.push(Error {
                        error_type: ErrorType::NoPrimaryKey {
                            record: name.clone(),
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

            _ => {}
        }
    }

    errors
}

fn check_schema_definitions(context: &Context, schem: &ast::Schema, mut errors: &mut Vec<Error>) {
    let vars = context.variants.clone();
    for (variant_name, mut instances) in vars {
        if instances.len() > 1 {
            let base_variant = instances.remove(0); // remove and use the first variant as the base
            let duplicates = instances; // the rest are duplicates

            errors.push(Error {
                error_type: ErrorType::DuplicateVariant {
                    base_variant,
                    duplicates,
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

    // Check definitions
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

enum ParamUsage {
    Defined,
    Used,
    NotDefinedButUsed,
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
    let mut param_names = HashMap::new();

    // Param types make sense?
    for param_def in &query.args {
        match context.types.get(&param_def.type_) {
            None => errors.push(Error {
                error_type: ErrorType::UnknownType(param_def.type_.clone()),
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            }),
            Some(_) => {}
        }

        param_names.insert(
            param_def.name.clone(),
            (ParamUsage::Defined, param_def.type_.clone()),
        );
    }

    // Check fields
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
            Some(table) => check_table_query(
                context,
                errors,
                &query.operation,
                table,
                field,
                &mut param_names,
            ),
        }
    }

    for (param_name, (usage, type_string)) in param_names {
        match usage {
            ParamUsage::Defined => errors.push(Error {
                error_type: ErrorType::UnusedParam { param: param_name },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            }),
            ParamUsage::Used => {}
            ParamUsage::NotDefinedButUsed => errors.push(Error {
                error_type: ErrorType::UndefinedParam {
                    param: param_name,
                    type_: type_string,
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            }),
            _ => {}
        }
    }
}

fn check_where_args(
    context: &Context,
    table: &ast::RecordDetails,
    errors: &mut Vec<Error>,
    params: &mut HashMap<String, (ParamUsage, String)>,
    where_args: &ast::WhereArg,
) {
    match where_args {
        ast::WhereArg::And(ands) => {
            for and in ands {
                check_where_args(context, table, errors, params, and);
            }
        }
        ast::WhereArg::Or(ors) => {
            for or in ors {
                check_where_args(context, table, errors, params, or);
            }
        }
        ast::WhereArg::Column(field_name, operator, query_val) => {
            let mut is_known_field = false;
            let mut column_type: Option<String> = None;
            for col in &table.fields {
                if is_known_field {
                    continue;
                }
                match col {
                    ast::Field::Column(column) => {
                        if &column.name == field_name {
                            is_known_field = true;
                            column_type = Some(column.type_.clone());
                        }
                    }
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        if &link.link_name == field_name {
                            is_known_field = true;
                            errors.push(Error {
                                error_type: ErrorType::WhereOnLinkIsntAllowed {
                                    link_name: field_name.clone(),
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
                    _ => (),
                }
            }
            if (!is_known_field) {
                errors.push(Error {
                    error_type: ErrorType::UnknownField {
                        found: field_name.clone(),
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

            match column_type {
                None => (),
                Some(column_type_string) => {
                    check_value(query_val, errors, params, &table.name, &column_type_string);
                }
            }

            // Check if the field exists
            // Get field type
        }
    }
}

fn check_value(
    value: &ast::QueryValue,
    errors: &mut Vec<Error>,
    params: &mut HashMap<String, (ParamUsage, String)>,
    table_name: &str,
    table_type_string: &str,
) {
    match value {
        ast::QueryValue::Variable(var) => match params.get_mut(var) {
            None => {
                errors.push(Error {
                    error_type: ErrorType::UndeclaredVariable {
                        variable: var.clone(),
                    },
                    location: Location {
                        highlight: None,
                        area: Range {
                            start: Coord { line: 0, column: 0 },
                            end: Coord { line: 0, column: 0 },
                        },
                    },
                });
                params.insert(
                    var.to_string(),
                    (ParamUsage::NotDefinedButUsed, table_type_string.to_string()),
                );
            }
            Some((usage, type_string)) => {
                if table_type_string != *type_string {
                    errors.push(Error {
                        error_type: ErrorType::TypeMismatch {
                            table: table_name.to_string(),
                            column_defined_as: table_type_string.to_string(),
                            variable_name: var.clone(),
                            variable_defined_as: type_string.clone(),
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
                match usage {
                    ParamUsage::Defined => *usage = ParamUsage::Used,
                    ParamUsage::Used => {}
                    ParamUsage::NotDefinedButUsed => {}
                }
            }
        },
        _ => {}
    }
}

fn check_table_query(
    context: &Context,
    mut errors: &mut Vec<Error>,
    operation: &ast::QueryOperation,
    table: &ast::RecordDetails,
    query: &ast::QueryField,
    params: &mut HashMap<String, (ParamUsage, String)>,
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

    let mut queried_fields: HashMap<String, bool> = HashMap::new();
    let mut has_limit = false;
    let mut has_offset = false;
    let mut has_where = false;

    // We've already checked that the top-level query field name is valid
    // we want to make sure that every field queried exists in `table` as a column
    for arg_field in &query.fields {
        match arg_field {
            ast::ArgField::Line { .. } => (),
            ast::ArgField::Arg(arg) => match arg {
                ast::Arg::Limit(limit_val) => {
                    if has_limit {
                        errors.push(Error {
                            error_type: ErrorType::MultipleLimits {
                                query: query.name.clone(),
                            },
                            location: Location {
                                highlight: None,
                                area: Range {
                                    start: Coord { line: 0, column: 0 },
                                    end: Coord { line: 0, column: 0 },
                                },
                            },
                        });
                    } else {
                        has_limit = true;
                    }

                    check_value(limit_val, errors, params, &table.name, "Int");
                }
                ast::Arg::Offset(offset_value) => {
                    if has_offset {
                        errors.push(Error {
                            error_type: ErrorType::MultipleOffsets {
                                query: query.name.clone(),
                            },
                            location: Location {
                                highlight: None,
                                area: Range {
                                    start: Coord { line: 0, column: 0 },
                                    end: Coord { line: 0, column: 0 },
                                },
                            },
                        });
                    } else {
                        has_offset = true;
                    }

                    check_value(offset_value, errors, params, &table.name, "Int");
                }
                ast::Arg::Where(whereArgs) => {
                    if has_where {
                        errors.push(Error {
                            error_type: ErrorType::MultipleWheres {
                                query: query.name.clone(),
                            },
                            location: Location {
                                highlight: None,
                                area: Range {
                                    start: Coord { line: 0, column: 0 },
                                    end: Coord { line: 0, column: 0 },
                                },
                            },
                        });
                    } else {
                        has_where = true;
                    }

                    check_where_args(context, table, errors, params, whereArgs);
                }
                _ => (),
            },
            ast::ArgField::Field(field) => {
                let aliased_name = ast::get_aliased_name(field);

                if queried_fields.get(&aliased_name).is_some() {
                    errors.push(Error {
                        error_type: ErrorType::DuplicateQueryField {
                            query: table.name.clone(),
                            field: aliased_name.clone(),
                        },
                        location: Location {
                            highlight: None,
                            area: Range {
                                start: Coord { line: 0, column: 0 },
                                end: Coord { line: 0, column: 0 },
                            },
                        },
                    });
                } else {
                    queried_fields.insert(aliased_name.clone(), field.set.is_some());
                }

                let mut is_known_field = false;
                for col in &table.fields {
                    if is_known_field {
                        continue;
                    }
                    match col {
                        ast::Field::Column(column) => {
                            if column.name == field.name {
                                is_known_field = true;
                                check_field(context, operation, errors, column, field)
                            }
                        }
                        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                            if link.link_name == field.name {
                                is_known_field = true;
                                check_link(context, operation, errors, link, field, params)
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
    }

    match operation {
        ast::QueryOperation::Insert => {
            for col in ast::collect_columns(&table.fields) {
                match queried_fields.get(&col.name) {
                    Some(is_set) => {
                        if (!is_set) {
                            errors.push(Error {
                                error_type: ErrorType::InsertColumnIsNotSet {
                                    field: col.name.clone(),
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
                    None => errors.push(Error {
                        error_type: ErrorType::InsertMissingColumn {
                            field: col.name.clone(),
                        },
                        location: Location {
                            highlight: None,
                            area: Range {
                                start: Coord { line: 0, column: 0 },
                                end: Coord { line: 0, column: 0 },
                            },
                        },
                    }),
                }
            }
        }
        _ => {}
    }
}

fn check_field(
    context: &Context,
    operation: &ast::QueryOperation,
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
    match operation {
        ast::QueryOperation::Select => {
            if (field.set.is_some()) {
                errors.push(Error {
                    error_type: ErrorType::NoSetsInSelect {
                        field: column.name.clone(),
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
        ast::QueryOperation::Insert => {
            // Set is required
            if (field.set.is_none()) {
                errors.push(Error {
                    error_type: ErrorType::MissingSetInInsert {
                        field: column.name.clone(),
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
        ast::QueryOperation::Update => {
            // Set is optional
        }
        ast::QueryOperation::Delete => {
            // Setting is disallowed
            if (field.set.is_some()) {
                errors.push(Error {
                    error_type: ErrorType::NoSetsInDelete {
                        field: column.name.clone(),
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
}

fn check_link(
    context: &Context,
    operation: &ast::QueryOperation,
    mut errors: &mut Vec<Error>,
    link: &ast::LinkDetails,
    field: &ast::QueryField,
    params: &mut HashMap<String, (ParamUsage, String)>,
) {
    // Links are only allowed in selects at the moment
    match operation {
        ast::QueryOperation::Insert => {
            errors.push(Error {
                error_type: ErrorType::LinksDisallowedInInserts {
                    field: link.link_name.clone(),
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            });
            return ();
        }
        ast::QueryOperation::Update => {
            errors.push(Error {
                error_type: ErrorType::LinksDisallowedInUpdates {
                    field: link.link_name.clone(),
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
        ast::QueryOperation::Delete => {
            errors.push(Error {
                error_type: ErrorType::LinksDisallowedInDeletes {
                    field: link.link_name.clone(),
                },
                location: Location {
                    highlight: None,
                    area: Range {
                        start: Coord { line: 0, column: 0 },
                        end: Coord { line: 0, column: 0 },
                    },
                },
            });
            return ();
        }
        _ => (),
    }

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
            Some(table) => check_table_query(context, errors, operation, table, field, params),
        }
    }
}
