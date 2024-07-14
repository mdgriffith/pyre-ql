use crate::ast;
use crate::error::{DefInfo, Error, ErrorType, Location, Range, VariantDef};
use crate::ext::string;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Deserialize, Serialize)]
pub struct Context {
    pub current_filepath: String,
    pub types: HashMap<String, DefInfo>,
    pub tables: HashMap<String, ast::RecordDetails>,
    pub variants: HashMap<String, (Option<Range>, Vec<VariantDef>)>,
}

fn convert_range(range: &ast::Range) -> Range {
    Range {
        start: range.start.clone(),
        end: range.end.clone(),
    }
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
        current_filepath: "".to_string(),
        types: HashMap::new(),
        tables: HashMap::new(),
        variants: HashMap::new(),
    };
    context.types.insert("String".to_string(), DefInfo::Builtin);
    context.types.insert("Int".to_string(), DefInfo::Builtin);
    context.types.insert("Float".to_string(), DefInfo::Builtin);
    context.types.insert("Bool".to_string(), DefInfo::Builtin);
    context
        .types
        .insert("DateTime".to_string(), DefInfo::Builtin);

    context
}

pub fn check_schema(schem: &ast::Schema) -> Result<Context, Vec<Error>> {
    let context = populate_context(schem)?;

    let mut errors: Vec<Error> = Vec::new();
    check_schema_definitions(&context, schem, &mut errors);

    if (!errors.is_empty()) {
        return Err(errors);
    }

    Ok(context)
}

fn to_range(start: &Option<ast::Location>, end: &Option<ast::Location>) -> Vec<Range> {
    match start {
        None => vec![],
        Some(s) => match end {
            None => vec![],
            Some(e) => vec![
                (Range {
                    start: s.clone(),
                    end: e.clone(),
                }),
            ],
        },
    }
}

fn to_single_range(start: &Option<ast::Location>, end: &Option<ast::Location>) -> Option<Range> {
    match start {
        None => None,
        Some(s) => match end {
            None => None,
            Some(e) => Some(Range {
                start: s.clone(),
                end: e.clone(),
            }),
        },
    }
}

pub fn populate_context(schem: &ast::Schema) -> Result<Context, Vec<Error>> {
    let mut context = empty_context();
    let mut errors = Vec::new();

    for file in schem.files.iter() {
        for definition in &file.definitions {
            match definition {
                ast::Definition::Record {
                    name,
                    fields,
                    start,
                    end,
                    start_name,
                    end_name,
                } => {
                    match context.types.get(name) {
                        None => (),
                        Some(DefInfo::Def(loc)) => {
                            let mut locations: Vec<Location> = vec![];
                            locations.push(Location {
                                contexts: vec![],
                                primary: loc.clone().into_iter().collect(),
                            });
                            locations.push(Location {
                                contexts: vec![],
                                primary: to_range(start_name, end_name),
                            });
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::DuplicateDefinition(name.clone()),
                                locations,
                            });
                        }
                        Some(DefInfo::Builtin) => {
                            let mut locations: Vec<Location> = vec![];
                            locations.push(Location {
                                contexts: vec![],
                                primary: to_range(start_name, end_name),
                            });
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::DefinitionIsBuiltIn(name.clone()),
                                locations,
                            });
                        }
                    }
                    context.types.insert(
                        name.clone(),
                        DefInfo::Def(to_single_range(start_name, end_name)),
                    );
                    context.tables.insert(
                        crate::ext::string::decapitalize(&name),
                        ast::RecordDetails {
                            name: name.clone(),
                            fields: fields.clone(),
                            start: start.clone(),
                            end: end.clone(),
                            start_name: start_name.clone(),
                            end_name: end_name.clone(),
                        },
                    );
                }
                ast::Definition::Tagged {
                    name,
                    variants,
                    start,
                    end,
                } => {
                    match context.types.get(name) {
                        None => (),
                        Some(DefInfo::Def(loc)) => {
                            let mut locations: Vec<Location> = vec![];
                            locations.push(Location {
                                contexts: vec![],
                                primary: loc.clone().into_iter().collect(),
                            });
                            locations.push(Location {
                                contexts: vec![],
                                primary: to_range(start, end),
                            });
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::DuplicateDefinition(name.clone()),
                                locations,
                            });
                        }
                        Some(DefInfo::Builtin) => {
                            let mut locations: Vec<Location> = vec![];
                            locations.push(Location {
                                contexts: vec![],
                                primary: to_range(start, end),
                            });
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::DefinitionIsBuiltIn(name.clone()),
                                locations,
                            });
                        }
                    }
                    context
                        .types
                        .insert(name.clone(), DefInfo::Def(to_single_range(start, end)));

                    for mut variant in variants {
                        let variant_def = VariantDef {
                            typename: name.clone(),
                            variant_name: variant.name.clone(),
                            range: to_single_range(&variant.start_name, &variant.end_name),
                        };

                        let type_range = to_single_range(&start, &end);

                        context
                            .variants
                            .entry(variant.name.clone())
                            .or_insert_with(|| (type_range, Vec::new()))
                            .1
                            .push(variant_def);
                    }
                }
                _ => {}
            }
        }
    }

    for file in schem.files.iter() {
        for definition in &file.definitions {
            match definition {
                ast::Definition::Record {
                    name,
                    fields,
                    start,
                    end,
                    start_name,
                    end_name,
                } => {
                    let mut tablenames: Vec<Range> = Vec::new();
                    let mut has_primary_id = false;
                    let mut field_names = HashSet::new();

                    for field in fields {
                        match field {
                            ast::Field::Column(column) => {
                                if field_names.contains(&column.name) {
                                    errors.push(Error {
                                        filepath: file.path.clone(),
                                        error_type: ErrorType::DuplicateField {
                                            record: name.clone(),
                                            field: column.name.clone(),
                                        },
                                        locations: vec![Location {
                                            contexts: to_range(&start, &end),
                                            primary: to_range(&column.start, &column.end),
                                        }],
                                    });
                                }
                                if (column
                                    .directives
                                    .iter()
                                    .any(|item| *item == ast::ColumnDirective::PrimaryKey))
                                {
                                    if has_primary_id {
                                        errors.push(Error {
                                            filepath: file.path.clone(),
                                            error_type: ErrorType::MultiplePrimaryKeys {
                                                record: name.clone(),
                                                field: name.clone(),
                                            },
                                            locations: vec![Location {
                                                contexts: to_range(&start, &end),
                                                primary: to_range(
                                                    &column.start_name,
                                                    &column.end_name,
                                                ),
                                            }],
                                        });
                                    }
                                    has_primary_id = true;
                                }

                                field_names.insert(name.clone());
                            }
                            ast::Field::FieldDirective(ast::FieldDirective::TableName((
                                tablename_range,
                                tablename,
                            ))) => tablenames.push(convert_range(tablename_range)),
                            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                                let maybe_foreign_table = get_linked_table(&context, link);

                                if field_names.contains(&link.link_name) {
                                    errors.push(Error {
                                        filepath: file.path.clone(),
                                        error_type: ErrorType::DuplicateField {
                                            record: name.clone(),
                                            field: link.link_name.clone(),
                                        },
                                        locations: vec![Location {
                                            contexts: vec![],
                                            primary: to_range(&start, &end),
                                        }],
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
                                                    filepath: file.path.clone(),
                                                    error_type:
                                                        ErrorType::LinkToUnknownForeignField {
                                                            link_name: link.link_name.clone(),
                                                            foreign_table: link
                                                                .foreign_tablename
                                                                .clone(),
                                                            unknown_foreign_field: foreign_id
                                                                .clone(),
                                                        },
                                                    locations: vec![Location {
                                                        contexts: to_range(&start, &end),
                                                        primary: to_range(
                                                            &link.start_name,
                                                            &link.end_name,
                                                        ),
                                                    }],
                                                });
                                            }
                                        }
                                    }
                                    None => {
                                        errors.push(Error {
                                            filepath: file.path.clone(),
                                            error_type: ErrorType::LinkToUnknownTable {
                                                link_name: link.link_name.clone(),
                                                unknown_table: link.foreign_tablename.clone(),
                                            },
                                            locations: vec![Location {
                                                contexts: to_range(&start, &end),
                                                primary: to_range(&link.start_name, &link.end_name),
                                            }],
                                        });
                                    }
                                }

                                // Check that the local ids exist
                                for local_id in &link.local_ids {
                                    if !fields.iter().any(|f| ast::has_fieldname(f, local_id)) {
                                        errors.push(Error {
                                            filepath: file.path.clone(),
                                            error_type: ErrorType::LinkToUnknownField {
                                                link_name: link.link_name.clone(),
                                                unknown_local_field: local_id.clone(),
                                            },
                                            locations: vec![Location {
                                                contexts: to_range(&start, &end),
                                                primary: to_range(&link.start_name, &link.end_name),
                                            }],
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    if tablenames.len() > 1 {
                        errors.push(Error {
                            filepath: file.path.clone(),
                            error_type: ErrorType::MultipleTableNames {
                                record: name.clone(),
                            },
                            locations: vec![Location {
                                contexts: to_range(&start, &end),
                                primary: tablenames,
                            }],
                        });
                    }

                    if !has_primary_id {
                        errors.push(Error {
                            filepath: file.path.clone(),
                            error_type: ErrorType::NoPrimaryKey {
                                record: name.clone(),
                            },
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&start_name, &end_name),
                            }],
                        });
                    }
                }

                _ => {}
            }
        }
    }

    if errors.len() > 0 {
        return Err(errors);
    } else {
        return Ok(context);
    }
}

fn check_schema_definitions(context: &Context, schem: &ast::Schema, mut errors: &mut Vec<Error>) {
    let vars = context.variants.clone();
    for (variant_name, (maybe_type_range, mut instances)) in vars {
        if instances.len() > 1 {
            let base_variant = instances.remove(0); // remove and use the first variant as the base
            let duplicates = instances; // the rest are duplicates
            let maybe_location = base_variant.range.clone();
            let mut primary_ranges: Vec<Range> = vec![];
            match maybe_location {
                None => (),
                Some(loc) => primary_ranges.push(loc),
            }

            for dup in &duplicates {
                match &dup.range {
                    None => (),
                    Some(range) => primary_ranges.push(range.clone()),
                }
            }

            let mut contexts = vec![];
            match maybe_type_range {
                None => (),
                Some(range) => contexts.push(range),
            }

            errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::DuplicateVariant {
                    base_variant,
                    duplicates,
                },
                locations: vec![Location {
                    contexts,
                    primary: primary_ranges,
                }],
            });
        }
    }

    // Check definitions
    for file in schem.files.iter() {
        for definition in &file.definitions {
            match definition {
                ast::Definition::Record {
                    name,
                    fields,
                    start,
                    end,
                    start_name,
                    end_name,
                } => {
                    let mut field_names: HashMap<String, Option<Range>> = HashMap::new();
                    for column in ast::collect_columns(fields) {
                        // Type exists check
                        if !context.types.contains_key(&column.type_) {
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::UnknownType {
                                    found: column.type_.clone(),
                                    known_types: context.types.keys().cloned().collect(),
                                },
                                locations: vec![Location {
                                    contexts: to_range(start, end),
                                    primary: to_range(&column.start, &column.end),
                                }],
                            });
                        }

                        // Duplicate field check
                        match field_names.get(&column.name) {
                            None => (),
                            Some(duplicate_maybe_range) => {
                                let mut ranges: Vec<Range> = vec![];

                                match duplicate_maybe_range {
                                    None => (),
                                    Some(new_range) => {
                                        ranges.push(new_range.clone());
                                    }
                                }

                                match to_single_range(&column.start_name, &column.end_name) {
                                    None => (),
                                    Some(new_range) => {
                                        ranges.push(new_range);
                                    }
                                }

                                errors.push(Error {
                                    filepath: file.path.clone(),
                                    error_type: ErrorType::DuplicateField {
                                        record: name.clone(),
                                        field: column.name.clone(),
                                    },
                                    locations: vec![Location {
                                        contexts: to_range(start, end),
                                        primary: ranges,
                                    }],
                                });
                            }
                        }
                        field_names.insert(
                            column.name.clone(),
                            to_single_range(&column.start_name, &column.end_name),
                        );
                    }
                }
                ast::Definition::Tagged {
                    name,
                    variants,
                    start,
                    end,
                } => {
                    for variant in variants {
                        if let Some(fields) = &variant.data {
                            for field in ast::collect_columns(&fields) {
                                if !context.types.contains_key(&field.type_) {
                                    let mut contexts: Vec<Range> = vec![];

                                    match to_single_range(&start, &end) {
                                        None => (),
                                        Some(new_range) => {
                                            contexts.push(new_range);
                                        }
                                    }

                                    match to_single_range(&variant.start, &variant.end) {
                                        None => (),
                                        Some(new_range) => {
                                            contexts.push(new_range);
                                        }
                                    }

                                    errors.push(Error {
                                        filepath: file.path.clone(),
                                        error_type: ErrorType::UnknownType {
                                            found: field.type_.clone(),
                                            known_types: context.types.keys().cloned().collect(),
                                        },
                                        locations: vec![Location {
                                            contexts,
                                            primary: to_range(
                                                &field.start_typename,
                                                &field.end_typename,
                                            ),
                                        }],
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// Check query

pub fn check_queries<'a>(
    schem: &ast::Schema,
    query_list: &ast::QueryList,
    context: &Context,
) -> Result<(), Vec<Error>> {
    let mut errors: Vec<Error> = Vec::new();
    check_schema_definitions(&context, schem, &mut errors);

    for mut query in &query_list.queries {
        match query {
            ast::QueryDef::Query(q) => {
                check_query(&context, &mut errors, &q);
                continue;
            }
            _ => continue,
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}

#[derive(Debug)]
pub enum ParamInfo {
    Defined {
        defined_at: Option<Range>,
        type_: Option<String>,
        used: bool,
        type_inferred: bool,
    },
    NotDefinedButUsed {
        used_at: Option<Range>,
        type_: Option<String>,
    },
}

pub fn check_query(
    context: &Context,
    mut errors: &mut Vec<Error>,
    query: &ast::Query,
) -> HashMap<String, ParamInfo> {
    // We need to check
    // 1. The field exists on the record in the schema
    //    What type is the field (add to `QueryField`)
    //
    // 2. If params are defined
    //     2.a All defined params are used
    //     2.b Every used param is defined
    //
    let mut param_names: HashMap<String, ParamInfo> = HashMap::new();

    // Param types make sense?
    for param_def in &query.args {
        match &param_def.type_ {
            None => {
                param_names.insert(
                    param_def.name.clone(),
                    ParamInfo::Defined {
                        defined_at: to_single_range(&param_def.start_name, &param_def.end_name),
                        type_: None,
                        used: false,
                        type_inferred: false,
                    },
                );
            }

            Some(type_) => {
                match context.types.get(type_) {
                    None => errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::UnknownType {
                            found: type_.clone(),
                            known_types: context.types.keys().cloned().collect(),
                        },
                        locations: vec![Location {
                            contexts: vec![],
                            primary: to_range(&param_def.start_type, &param_def.end_type),
                        }],
                    }),
                    Some(_) => {}
                }

                param_names.insert(
                    param_def.name.clone(),
                    ParamInfo::Defined {
                        defined_at: to_single_range(&param_def.start_name, &param_def.end_name),
                        type_: Some(type_.clone()),
                        used: false,
                        type_inferred: false,
                    },
                );
            }
        }
    }

    // Check fields
    for field in &query.fields {
        match context.tables.get(&field.name) {
            None => errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::UnknownTable {
                    found: field.name.clone(),
                    existing: vec![],
                },
                locations: vec![Location {
                    contexts: to_range(&query.start, &query.end),
                    primary: to_range(&field.start_fieldname, &field.end_fieldname),
                }],
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

    for (param_name, param_info) in param_names.iter() {
        match &param_info {
            ParamInfo::Defined {
                defined_at,
                type_,
                used,
                type_inferred,
            } => {
                if *used == false {
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::UnusedParam {
                            param: param_name.clone(),
                        },
                        locations: vec![Location {
                            contexts: vec![],
                            primary: defined_at.clone().into_iter().collect(),
                        }],
                    })
                } else if *type_inferred {
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::MissingType,
                        locations: vec![Location {
                            contexts: vec![],
                            primary: defined_at.clone().into_iter().collect(),
                        }],
                    })
                }
            }
            ParamInfo::NotDefinedButUsed { used_at, type_ } => errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::UndefinedParam {
                    param: param_name.clone(),
                    type_: type_.clone(),
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: used_at.clone().into_iter().collect(),
                }],
            }),
            _ => {}
        }
    }

    param_names
}

fn check_where_args(
    context: &Context,
    start: &Option<ast::Location>,
    end: &Option<ast::Location>,
    table: &ast::RecordDetails,
    errors: &mut Vec<Error>,
    params: &mut HashMap<String, ParamInfo>,
    where_args: &ast::WhereArg,
) {
    match where_args {
        ast::WhereArg::And(ands) => {
            for and in ands {
                check_where_args(context, start, end, table, errors, params, and);
            }
        }
        ast::WhereArg::Or(ors) => {
            for or in ors {
                check_where_args(context, start, end, table, errors, params, or);
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
                                filepath: context.current_filepath.clone(),
                                error_type: ErrorType::WhereOnLinkIsntAllowed {
                                    link_name: field_name.clone(),
                                },
                                locations: vec![Location {
                                    contexts: vec![],
                                    primary: to_range(&link.start_name, &link.end_name),
                                }],
                            })
                        }
                    }
                    _ => (),
                }
            }
            if (!is_known_field) {
                let known_fields = get_column_reference(&table.fields);
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::UnknownField {
                        found: field_name.clone(),

                        record_name: table.name.clone(),
                        known_fields,
                    },
                    locations: vec![],
                })
            }

            match column_type {
                None => mark_as_used(query_val, errors, params),
                Some(column_type_string) => {
                    check_value(
                        context,
                        query_val,
                        start,
                        end,
                        errors,
                        params,
                        &table.name,
                        &column_type_string,
                    );
                }
            }

            // Check if the field exists
            // Get field type
        }
    }
}

fn mark_as_used(
    value: &ast::QueryValue,
    errors: &mut Vec<Error>,
    params: &mut HashMap<String, ParamInfo>,
) {
    match value {
        ast::QueryValue::Variable((var_range, var)) => match params.get_mut(var) {
            None => {
                params.insert(
                    var.to_string(),
                    ParamInfo::NotDefinedButUsed {
                        used_at: Some(convert_range(var_range)),
                        type_: None,
                    },
                );
            }
            Some(param_info) => {
                match param_info {
                    ParamInfo::Defined {
                        defined_at,
                        ref mut type_,
                        mut used,
                        mut type_inferred,
                    } => {
                        // mark as used
                        used = true;
                    }
                    ParamInfo::NotDefinedButUsed { used_at, type_ } => (),
                };
            }
        },
        _ => {}
    }
}

fn check_value(
    context: &Context,
    value: &ast::QueryValue,
    start: &Option<ast::Location>,
    end: &Option<ast::Location>,
    errors: &mut Vec<Error>,
    params: &mut HashMap<String, ParamInfo>,
    table_name: &str,
    table_type_string: &str,
) {
    match value {
        ast::QueryValue::Variable((var_range, var)) => match params.get_mut(var) {
            None => {
                params.insert(
                    var.to_string(),
                    ParamInfo::NotDefinedButUsed {
                        used_at: Some(convert_range(var_range)),
                        type_: Some(table_type_string.to_string()),
                    },
                );
            }
            Some(param_info) => {
                match param_info {
                    ParamInfo::Defined {
                        defined_at,
                        ref mut type_,
                        ref mut used,
                        ref mut type_inferred,
                    } => {
                        // mark as used
                        *used = true;

                        match &type_ {
                            None => {
                                // We can set the type, but also mark it as inferred
                                // If it's inferred, it will error if exec'ed, but succeed if formatted
                                *type_ = Some(table_type_string.to_string());
                                *type_inferred = true;
                            }
                            Some(type_name) => {
                                if type_name != table_type_string {
                                    errors.push(Error {
                                        filepath: context.current_filepath.clone(),
                                        error_type: ErrorType::TypeMismatch {
                                            table: table_name.to_string(),
                                            column_defined_as: table_type_string.to_string(),
                                            variable_name: var.clone(),
                                            variable_defined_as: type_name.clone(),
                                        },
                                        locations: vec![
                                            Location {
                                                contexts: vec![],
                                                primary: defined_at
                                                    .as_ref()
                                                    .map_or_else(Vec::new, |range| {
                                                        vec![range.clone()]
                                                    }),
                                            },
                                            Location {
                                                contexts: vec![], // to_range(&start, &end),
                                                primary: vec![convert_range(var_range)],
                                            },
                                        ],
                                    })
                                }
                            }
                        }
                    }
                    ParamInfo::NotDefinedButUsed { used_at, type_ } => (),
                };
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
    params: &mut HashMap<String, ParamInfo>,
) {
    if query.fields.is_empty() {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::NoFieldsSelected,
            locations: vec![Location {
                contexts: vec![],
                primary: to_range(&query.start_fieldname, &query.end_fieldname),
            }],
        })
    }

    let mut queried_fields: HashMap<String, bool> = HashMap::new();

    let mut limits: Vec<Range> = vec![];
    let mut offsets: Vec<Range> = vec![];
    let mut wheres: Vec<Range> = vec![];
    let mut has_nested_selected = false;

    // We've already checked that the top-level query field name is valid
    // we want to make sure that every field queried exists in `table` as a column
    for arg_field in &query.fields {
        match arg_field {
            ast::ArgField::Line { .. } => (),
            ast::ArgField::Arg(arg) => {
                let arg_data = &arg.arg;
                match arg_data {
                    ast::Arg::Limit(limit_val) => {
                        match to_single_range(&arg.start, &arg.end) {
                            Some(range) => limits.push(range),
                            None => (),
                        }

                        check_value(
                            context,
                            &limit_val,
                            &arg.start,
                            &arg.end,
                            errors,
                            params,
                            &table.name,
                            "Int",
                        );
                    }
                    ast::Arg::Offset(offset_value) => {
                        match to_single_range(&arg.start, &arg.end) {
                            Some(range) => offsets.push(range),
                            None => (),
                        }

                        check_value(
                            context,
                            &offset_value,
                            &arg.start,
                            &arg.end,
                            errors,
                            params,
                            &table.name,
                            "Int",
                        );
                    }
                    ast::Arg::Where(whereArgs) => {
                        match to_single_range(&arg.start, &arg.end) {
                            Some(range) => wheres.push(range),
                            None => (),
                        }

                        check_where_args(
                            context, &arg.start, &arg.end, table, errors, params, &whereArgs,
                        );
                    }
                    _ => (),
                }
            }
            ast::ArgField::Field(field) => {
                let aliased_name = ast::get_aliased_name(field);

                if queried_fields.get(&aliased_name).is_some() {
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::DuplicateQueryField {
                            query: table.name.clone(),
                            field: aliased_name.clone(),
                        },
                        locations: vec![Location {
                            contexts: to_range(&query.start, &query.end),
                            primary: to_range(&field.start_fieldname, &field.end_fieldname),
                        }],
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
                                check_field(context, params, operation, errors, column, field)
                            }
                        }
                        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                            if link.link_name == field.name {
                                is_known_field = true;
                                has_nested_selected = true;
                                check_link(context, operation, errors, link, field, params)
                            }
                            ()
                        }
                        _ => (),
                    }
                }
                if (!is_known_field) {
                    let known_fields = get_column_reference(&table.fields);
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::UnknownField {
                            found: field.name.clone(),
                            record_name: table.name.clone(),
                            known_fields,
                        },
                        locations: vec![Location {
                            contexts: to_range(&query.start, &query.end),
                            primary: to_range(&field.start_fieldname, &field.end_fieldname),
                        }],
                    })
                }
            }
        }
    }

    let limit_len = limits.len();
    if limit_len > 1 {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::MultipleLimits {
                query: query.name.clone(),
            },
            locations: vec![Location {
                contexts: to_range(&query.start, &query.end),
                primary: limits.clone(),
            }],
        });
    }
    let offset_len = offsets.len();
    if offset_len > 1 {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::MultipleOffsets {
                query: query.name.clone(),
            },
            locations: vec![Location {
                contexts: to_range(&query.start, &query.end),
                primary: offsets.clone(),
            }],
        });
    }

    if ((offset_len > 0 || limit_len > 0) && has_nested_selected) {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::LimitOffsetOnlyInFlatRecord,
            locations: vec![Location {
                contexts: to_range(&query.start, &query.end),
                primary: [limits, offsets].concat(),
            }],
        });
    }

    if wheres.len() > 1 {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::MultipleWheres {
                query: query.name.clone(),
            },
            locations: vec![Location {
                contexts: to_range(&query.start, &query.end),
                primary: wheres,
            }],
        });
    }

    match operation {
        ast::QueryOperation::Insert => {
            for col in ast::collect_columns(&table.fields) {
                if ast::is_primary_key(&col) || ast::has_default_value(&col) {
                    // Primary keys aren't required and those with defaults aren't required
                    // (for the moment, we should differentiate between auto-incrementing
                    // and non-auto-incrementing primary keys)
                    continue;
                }
                match queried_fields.get(&col.name) {
                    Some(is_set) => {
                        if (!is_set) {
                            errors.push(Error {
                                filepath: context.current_filepath.clone(),
                                error_type: ErrorType::InsertColumnIsNotSet {
                                    field: col.name.clone(),
                                },
                                locations: vec![Location {
                                    contexts: vec![],
                                    primary: to_range(&query.start, &query.end),
                                }],
                            })
                        }
                    }
                    None => errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::InsertMissingColumn {
                            field: col.name.clone(),
                        },
                        locations: vec![Location {
                            contexts: vec![],
                            primary: to_range(&query.start, &query.end),
                        }],
                    }),
                }
            }
        }
        _ => {}
    }
}

fn check_field(
    context: &Context,
    params: &mut HashMap<String, ParamInfo>,
    operation: &ast::QueryOperation,
    mut errors: &mut Vec<Error>,
    column: &ast::Column,
    field: &ast::QueryField,
) {
    // TODO::I think the below is if you're selecting a field like a link
    // if (!field.fields.is_empty()) {
    //     let mut known_fields: Vec<(String, String)> = vec![];
    //     for col in &field.fields {}
    //     errors.push(Error {
    //         error_type: ErrorType::UnknownField {
    //             found: field.name.clone(),
    //             record_name: field.name,
    //             known_fields,
    //         },
    //         location: Location {
    //             contexts: vec![],
    //             primary: to_range(&field.start, &field.end),
    //         },
    //     })
    // }
    match &field.set {
        Some(set) => {
            check_value(
                context,
                &set,
                &field.start,
                &field.end,
                &mut errors,
                params,
                &column.name,
                &column.type_,
            );
        }
        None => {}
    }

    match operation {
        ast::QueryOperation::Select => {
            if (field.set.is_some()) {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::NoSetsInSelect {
                        field: column.name.clone(),
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&field.start_fieldname, &field.end_fieldname),
                    }],
                })
            }
        }
        ast::QueryOperation::Insert => {
            // Set is required
        }
        ast::QueryOperation::Update => {
            // Set is optional
        }
        ast::QueryOperation::Delete => {
            // Setting is disallowed
            if (field.set.is_some()) {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::NoSetsInDelete {
                        field: column.name.clone(),
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&field.start_fieldname, &field.end_fieldname),
                    }],
                })
            }
        }
    }
}

fn get_column_reference(fields: &Vec<ast::Field>) -> Vec<(String, String)> {
    let mut known_fields: Vec<(String, String)> = vec![];
    for col in fields {
        match col {
            ast::Field::Column(column) => {
                known_fields.push((column.name.clone(), column.type_.clone()))
            }
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                known_fields.push((link.link_name.clone(), link.foreign_tablename.clone()))
            }
            _ => (),
        }
    }
    known_fields
}

fn check_link(
    context: &Context,
    operation: &ast::QueryOperation,
    mut errors: &mut Vec<Error>,
    link: &ast::LinkDetails,
    field: &ast::QueryField,
    params: &mut HashMap<String, ParamInfo>,
) {
    // Links are only allowed in selects at the moment
    match operation {
        ast::QueryOperation::Insert => {
            errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::LinksDisallowedInInserts {
                    field: link.link_name.clone(),
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            });
            return ();
        }
        ast::QueryOperation::Update => {
            errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::LinksDisallowedInUpdates {
                    field: link.link_name.clone(),
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            });
        }
        ast::QueryOperation::Delete => {
            errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::LinksDisallowedInDeletes {
                    field: link.link_name.clone(),
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            });
            return ();
        }
        _ => (),
    }

    if (field.fields.is_empty()) {
        let mut known_fields: Vec<(String, String)> = vec![];

        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::LinkSelectionIsEmpty {
                link_name: link.link_name.clone(),
                foreign_table: link.foreign_tablename.clone(),
                foreign_table_fields: known_fields,
            },
            locations: vec![Location {
                // contexts: to_range(&field.start, &field.end),
                contexts: vec![],
                primary: to_range(&field.start_fieldname, &field.end_fieldname),
            }],
        })
    } else {
        let table = context
            .tables
            .get(&crate::ext::string::decapitalize(&link.foreign_tablename));
        match table {
            None => errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::UnknownTable {
                    found: link.foreign_tablename.clone(),
                    existing: vec![],
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            }),
            Some(table) => check_table_query(context, errors, operation, table, field, params),
        }
    }
}
