use crate::ext::string;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast;

#[derive(Debug, Deserialize, Serialize)]
pub struct Error {
    pub error_type: ErrorType,
    pub locations: Vec<Location>,
}

/*


    For trakcing location errors, we have a few different considerations.

    1. Generally a language server takes a single range, so that should easily be retrievable.
    2. For error rendering in the terminal, we want a hierarchy of the contexts we're in.
        So, we want
            - The Query
            - The table field, etc.

*/

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Location {
    pub contexts: Vec<Range>,
    pub primary: Option<Range>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Range {
    pub start: ast::Location,
    pub end: ast::Location,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ErrorType {
    DuplicateDefinition(String),
    DefinitionIsBuiltIn(String),
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
    LinkSelectionIsEmpty {
        link_name: String,
        known_fields: Vec<(String, String)>,
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

        record_name: String,
        known_fields: Vec<(String, String)>,
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

#[derive(Debug, Deserialize, Serialize)]
enum DefInfo {
    Def(Option<Range>),
    Builtin,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Context {
    pub types: HashMap<String, DefInfo>,
    pub tables: HashMap<String, ast::RecordDetails>,
    pub variants: HashMap<String, Vec<VariantDef>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    context.types.insert("String".to_string(), DefInfo::Builtin);
    context.types.insert("Int".to_string(), DefInfo::Builtin);
    context.types.insert("Float".to_string(), DefInfo::Builtin);
    context.types.insert("Bool".to_string(), DefInfo::Builtin);
    context
        .types
        .insert("DateTime".to_string(), DefInfo::Builtin);

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

    if (!errors.is_empty()) {
        return Err(errors);
    }

    Ok(schem)
}

fn to_highlight_range(start: &Option<ast::Location>, end: &Option<ast::Location>) -> Vec<Range> {
    match to_range(start, end) {
        None => vec![],
        Some(item) => vec![item],
    }
}

fn to_range(start: &Option<ast::Location>, end: &Option<ast::Location>) -> Option<Range> {
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

fn populate_context(schem: &ast::Schema, context: &mut Context) -> Vec<Error> {
    let mut errors = Vec::new();

    for definition in &schem.definitions {
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
                            primary: loc.clone(),
                        });
                        locations.push(Location {
                            contexts: vec![],
                            primary: to_range(start_name, end_name),
                        });
                        errors.push(Error {
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
                            error_type: ErrorType::DefinitionIsBuiltIn(name.clone()),
                            locations,
                        });
                    }
                }
                context
                    .types
                    .insert(name.clone(), DefInfo::Def(to_range(start_name, end_name)));
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
                            primary: loc.clone(),
                        });
                        locations.push(Location {
                            contexts: vec![],
                            primary: to_range(start, end),
                        });
                        errors.push(Error {
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
                            error_type: ErrorType::DefinitionIsBuiltIn(name.clone()),
                            locations,
                        });
                    }
                }
                context
                    .types
                    .insert(name.clone(), DefInfo::Def(to_range(start, end)));

                for mut variant in variants {
                    let location = Location {
                        contexts: vec![],
                        primary: to_range(start, end),
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
            ast::Definition::Record {
                name,
                fields,
                start,
                end,
                start_name,
                end_name,
            } => {
                let mut tablenames: Vec<String> = Vec::new();
                let mut has_primary_id = false;
                let mut field_names = HashSet::new();

                for field in fields {
                    match field {
                        ast::Field::Column(column) => {
                            if field_names.contains(&column.name) {
                                errors.push(Error {
                                    error_type: ErrorType::DuplicateField {
                                        record: name.clone(),
                                        field: column.name.clone(),
                                    },
                                    locations: vec![Location {
                                        contexts: to_highlight_range(&start, &end),
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
                                        error_type: ErrorType::MultiplePrimaryKeys {
                                            record: name.clone(),
                                            field: name.clone(),
                                        },
                                        locations: vec![Location {
                                            contexts: vec![],
                                            primary: to_range(&start, &end),
                                        }],
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
                                                error_type: ErrorType::LinkToUnknownForeignField {
                                                    link_name: link.link_name.clone(),
                                                    unknown_foreign_field: foreign_id.clone(),
                                                },
                                                locations: vec![Location {
                                                    contexts: vec![],
                                                    primary: to_range(&start, &end),
                                                }],
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
                                        locations: vec![Location {
                                            contexts: vec![],
                                            primary: to_range(&start, &end),
                                        }],
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
                                        locations: vec![Location {
                                            contexts: vec![],
                                            primary: to_range(&start, &end),
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
                        error_type: ErrorType::MultipleTableNames {
                            record: name.clone(),
                            tablenames: tablenames.clone(),
                        },
                        locations: vec![Location {
                            contexts: vec![],
                            primary: to_range(&start, &end),
                        }],
                    });
                }

                if !has_primary_id {
                    errors.push(Error {
                        error_type: ErrorType::NoPrimaryKey {
                            record: name.clone(),
                        },
                        locations: vec![Location {
                            contexts: vec![],
                            primary: to_range(&start, &end),
                        }],
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
            let location = base_variant.location.clone();
            errors.push(Error {
                error_type: ErrorType::DuplicateVariant {
                    base_variant,
                    duplicates,
                },
                locations: vec![location],
            });
        }
    }

    // Check definitions
    for definition in &schem.definitions {
        match definition {
            ast::Definition::Record {
                name,
                fields,
                start,
                end,
                start_name,
                end_name,
            } => {
                let mut field_names = HashSet::new();
                for column in ast::collect_columns(fields) {
                    // Type exists check
                    if !context.types.contains_key(&column.type_) {
                        errors.push(Error {
                            error_type: ErrorType::UnknownType(column.type_.clone()),
                            locations: vec![Location {
                                contexts: to_highlight_range(start, end),
                                primary: to_range(&column.start, &column.end),
                            }],
                        });
                    }

                    // Duplicate field check
                    if field_names.contains(&column.name) {
                        errors.push(Error {
                            error_type: ErrorType::DuplicateField {
                                record: name.clone(),
                                field: column.name.clone(),
                            },
                            locations: vec![Location {
                                contexts: to_highlight_range(start, end),
                                primary: to_range(&column.start_name, &column.end_name),
                            }],
                        });
                    }
                    field_names.insert(column.name.clone());
                }
            }
            ast::Definition::Tagged {
                name,
                variants,
                start,
                end,
            } => {
                for variant in variants {
                    match variant {
                        ast::Variant {
                            name,
                            data,
                            start,
                            end,
                        } => {
                            if let Some(fields) = data {
                                for field in ast::collect_columns(fields) {
                                    if !context.types.contains_key(&field.type_) {
                                        errors.push(Error {
                                            error_type: ErrorType::UnknownType(field.type_.clone()),
                                            locations: vec![Location {
                                                contexts: vec![],
                                                primary: to_range(&start, &end),
                                            }],
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
    Defined(Option<Range>),
    Used,
    NotDefinedButUsed(Option<Range>),
}

struct ParamInfo {
    usage: ParamUsage,
    type_: String,
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
    let mut param_names: HashMap<String, (ParamUsage, String)> = HashMap::new();

    // Param types make sense?
    for param_def in &query.args {
        match context.types.get(&param_def.type_) {
            None => errors.push(Error {
                error_type: ErrorType::UnknownType(param_def.type_.clone()),
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&param_def.start_type, &param_def.end_type),
                }],
            }),
            Some(_) => {}
        }

        param_names.insert(
            param_def.name.clone(),
            (
                ParamUsage::Defined(to_range(&param_def.start_name, &param_def.end_name)),
                param_def.type_.clone(),
            ),
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
                locations: vec![Location {
                    contexts: to_highlight_range(&query.start, &query.end),
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

    for (param_name, (usage, type_string)) in param_names {
        match usage {
            ParamUsage::Defined(loc) => errors.push(Error {
                error_type: ErrorType::UnusedParam { param: param_name },
                locations: vec![Location {
                    contexts: vec![],
                    primary: loc,
                }],
            }),
            ParamUsage::Used => {}
            ParamUsage::NotDefinedButUsed(loc) => errors.push(Error {
                error_type: ErrorType::UndefinedParam {
                    param: param_name,
                    type_: type_string,
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: loc,
                }],
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
                                locations: vec![Location {
                                    contexts: vec![],
                                    primary: None,
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
                    error_type: ErrorType::UnknownField {
                        found: field_name.clone(),

                        record_name: table.name.clone(),
                        known_fields,
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: None,
                    }],
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
                    locations: vec![Location {
                        contexts: vec![],
                        primary: None,
                    }],
                });
                params.insert(
                    var.to_string(),
                    (
                        ParamUsage::NotDefinedButUsed(None),
                        table_type_string.to_string(),
                    ),
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
                        locations: vec![Location {
                            contexts: vec![],
                            primary: None,
                        }],
                    })
                }
                match usage {
                    ParamUsage::Defined(_) => *usage = ParamUsage::Used,
                    ParamUsage::Used => {}
                    ParamUsage::NotDefinedButUsed(_) => {}
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
            locations: vec![Location {
                contexts: vec![],
                primary: to_range(&query.start, &query.end),
            }],
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
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&query.start, &query.end),
                            }],
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
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&query.start, &query.end),
                            }],
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
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&query.start, &query.end),
                            }],
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
                        locations: vec![Location {
                            contexts: vec![],
                            primary: to_range(&query.start, &query.end),
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
                        error_type: ErrorType::UnknownField {
                            found: field.name.clone(),
                            record_name: table.name.clone(),
                            known_fields,
                        },
                        locations: vec![Location {
                            contexts: to_highlight_range(&query.start, &query.end),
                            primary: to_range(&field.start_fieldname, &field.end_fieldname),
                        }],
                    })
                }
            }
        }
    }

    match operation {
        ast::QueryOperation::Insert => {
            for col in ast::collect_columns(&table.fields) {
                if ast::is_primary_key(&col) {
                    // Primary keys aren't required
                    // (for the moment, we should differentiate between auto-incrementing
                    // and non-auto-incrementing primary keys)
                    continue;
                }
                match queried_fields.get(&col.name) {
                    Some(is_set) => {
                        if (!is_set) {
                            errors.push(Error {
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
    params: &mut HashMap<String, (ParamUsage, String)>,
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
            check_value(&set, &mut errors, params, &column.name, &column.type_);
        }
        None => {}
    }

    match operation {
        ast::QueryOperation::Select => {
            if (field.set.is_some()) {
                errors.push(Error {
                    error_type: ErrorType::NoSetsInSelect {
                        field: column.name.clone(),
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&field.start, &field.end),
                    }],
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
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&field.start, &field.end),
                    }],
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
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&field.start, &field.end),
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
    params: &mut HashMap<String, (ParamUsage, String)>,
) {
    // Links are only allowed in selects at the moment
    match operation {
        ast::QueryOperation::Insert => {
            errors.push(Error {
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
        // for col in &link.fields {

        // }

        errors.push(Error {
            error_type: ErrorType::LinkSelectionIsEmpty {
                link_name: link.link_name.clone(),
                known_fields,
            },
            locations: vec![Location {
                contexts: vec![],
                primary: to_range(&field.start, &field.end),
            }],
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
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            }),
            Some(table) => check_table_query(context, errors, operation, table, field, params),
        }
    }
}
