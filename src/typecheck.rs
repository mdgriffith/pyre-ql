use crate::error::{DefInfo, Error, ErrorType, Location, Range, VariantDef};
use crate::{ast, error, platform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Deserialize, Serialize)]
pub enum Type {
    Integer,
    Float,
    String,
    OneOf { variants: Vec<ast::Variant> },
    Record(ast::RecordDetails),
}

pub struct SqlColumnInfo {
    pub name: String,
    pub nullable: bool,
    pub type_: ast::ConcreteSerializationType,
    pub directives: Vec<ast::ColumnDirective>,
}

pub fn to_sql_column_info(context: &Context, fields: &Vec<ast::Field>) -> Vec<SqlColumnInfo> {
    let mut infos = vec![];

    for col in fields {
        field_to_sql_column(context, col, None, &mut infos);
    }
    infos
}

fn field_to_sql_column(
    context: &Context,
    field: &ast::Field,
    parent_name: Option<String>,
    gathered: &mut Vec<SqlColumnInfo>,
) {
    match field {
        ast::Field::Column(column) => match &column.serialization_type {
            ast::SerializationType::Concrete(concrete) => gathered.push(SqlColumnInfo {
                name: match parent_name {
                    None => column.name.clone(),
                    Some(prefix) => {
                        format!("{}{}", prefix, column.name)
                    }
                },

                nullable: column.nullable,
                type_: concrete.clone(),
                directives: column.directives.clone(),
            }),
            ast::SerializationType::FromType(typename) => {
                match context.types.get(typename) {
                    Some((_, type_)) => match type_ {
                        Type::OneOf { variants } => {
                            // We need a column for the discriminator
                            gathered.push(SqlColumnInfo {
                                name: column.name.clone(),
                                nullable: column.nullable,
                                type_: ast::ConcreteSerializationType::Text,
                                directives: column.directives.clone(),
                            });

                            let base_name = match parent_name {
                                None => &format!("{}__", column.name),
                                Some(parent) => &format!("{}__{}__", parent, column.name),
                            };
                            //
                            for var in variants {
                                // For each variant, we need to capture any additional fields
                                // Prefixed by the type name and variant name
                                // So, statusFieldName: Status.Active { activatedAt: DateTime }
                                // Turns into a colum  of statusFieldName__activatedAt
                                // Variants that have the same fields will share the same columns in the db
                                // If types are nested, then they'll be recursively namespaced by the fieldnames

                                match &var.fields {
                                    Some(var_fields) => {
                                        for var_f in var_fields {
                                            field_to_sql_column(
                                                context,
                                                &var_f,
                                                Some(base_name.to_string()),
                                                gathered,
                                            );
                                        }
                                    }
                                    None => continue,
                                }
                            }
                        }
                        _ => (),
                    },
                    None => {
                        // Should have been caught in typechecking
                    }
                }
            }
        },
        ast::Field::ColumnLines { .. } => (),
        ast::Field::ColumnComment { .. } => (),
        ast::Field::FieldDirective(_) => (),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Context {
    pub current_filepath: String,

    // Used to track which params are used in which
    // toplevel query field.
    pub top_level_field_alias: String,

    pub valid_namespaces: HashSet<String>,
    pub session: Option<ast::SessionDetails>,
    pub funcs: HashMap<String, platform::FuncDefinition>,

    pub types: HashMap<String, (DefInfo, Type)>,
    pub tables: HashMap<String, Table>,

    // All variants by their variant name
    // Used to check if there are multiple types with the same name.
    // Tracks the location of type definitions so they can be reported in the error.
    pub variants: HashMap<String, (Option<Range>, Vec<VariantDef>)>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Table {
    pub schema: String,
    pub record: ast::RecordDetails,
}

#[derive(Debug, Deserialize, Serialize)]
struct FuncDefinition {
    name: String,
    arg_types: Vec<String>,
    return_type: String,
}

fn convert_range(range: &ast::Range) -> Range {
    Range {
        start: range.start.clone(),
        end: range.end.clone(),
    }
}

pub fn get_linked_table<'a>(context: &'a Context, link: &'a ast::LinkDetails) -> Option<&'a Table> {
    context
        .tables
        .get(&crate::ext::string::decapitalize(&link.foreign.table))
}

fn empty_context() -> Context {
    let mut fns = HashMap::new();
    platform::add_builtin(&mut fns);

    let mut context = Context {
        current_filepath: "".to_string(),
        top_level_field_alias: "".to_string(),
        valid_namespaces: HashSet::new(),
        session: None,
        funcs: fns,
        types: HashMap::new(),
        tables: HashMap::new(),
        variants: HashMap::new(),
    };
    context
        .types
        .insert("String".to_string(), (DefInfo::Builtin, Type::String));
    context
        .types
        .insert("Int".to_string(), (DefInfo::Builtin, Type::Integer));
    context
        .types
        .insert("Float".to_string(), (DefInfo::Builtin, Type::Float));
    context.types.insert(
        "Bool".to_string(),
        (
            DefInfo::Builtin,
            Type::OneOf {
                variants: vec![ast::to_variant("True"), ast::to_variant("False")],
            },
        ),
    );
    context
        .types
        .insert("DateTime".to_string(), (DefInfo::Builtin, Type::String));

    context
}

fn is_capitalized(s: &str) -> bool {
    if let Some(first_char) = s.chars().next() {
        first_char.is_uppercase()
    } else {
        false
    }
}

pub fn check_schema(db: &ast::Database) -> Result<Context, Vec<Error>> {
    let context = populate_context(db)?;

    let mut errors: Vec<Error> = Vec::new();

    // Namespaces must be capitalized
    for schem in &db.schemas {
        if schem.namespace == ast::DEFAULT_SCHEMANAME {
            continue;
        } else if !is_capitalized(&schem.namespace) {
            let body = format!(
                "This schema name must be capitalized: {}",
                &error::yellow_if(true, &schem.namespace)
            );
            let error = error::format_custom_error("Schema name formatting", &body);
            eprintln!("{}", error);
            std::process::exit(1);
        }
    }

    check_schema_definitions(&context, db, &mut errors);

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

/// Gathers information for a context.
/// Also checks for a number of errors
pub fn populate_context(database: &ast::Database) -> Result<Context, Vec<Error>> {
    let mut context = empty_context();
    let mut errors = Vec::new();
    context.valid_namespaces = database
        .schemas
        .iter()
        .map(|schema| schema.namespace.clone())
        .collect();

    // Check for duplicate records
    // Check for duplicate types
    // Gather table names
    // Gather type names
    for schema in &database.schemas {
        if context.session.is_none() && schema.session.is_some() {
            context.session = schema.session.clone();
        }

        for file in &schema.files {
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
                            Some((DefInfo::Def(loc), _)) => {
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
                            Some((DefInfo::Builtin, _)) => {
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
                            (
                                DefInfo::Def(to_single_range(start_name, end_name)),
                                Type::Record(ast::RecordDetails {
                                    name: name.clone(),
                                    fields: fields.clone(),
                                    start: start.clone(),
                                    end: end.clone(),
                                    start_name: start_name.clone(),
                                    end_name: end_name.clone(),
                                }),
                            ),
                        );
                        context.tables.insert(
                            crate::ext::string::decapitalize(&name),
                            Table {
                                schema: schema.namespace.clone(),
                                record: ast::RecordDetails {
                                    name: name.clone(),
                                    fields: fields.clone(),
                                    start: start.clone(),
                                    end: end.clone(),
                                    start_name: start_name.clone(),
                                    end_name: end_name.clone(),
                                },
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
                            Some((DefInfo::Def(loc), _)) => {
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
                            Some((DefInfo::Builtin, _)) => {
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
                        context.types.insert(
                            name.clone(),
                            (
                                DefInfo::Def(to_single_range(start, end)),
                                Type::OneOf {
                                    variants: variants.clone(),
                                },
                            ),
                        );

                        for variant in variants {
                            let variant_def = VariantDef {
                                typename: name.clone(),
                                variant_name: variant.name.clone(),
                                range: to_single_range(&variant.start_name, &variant.end_name),
                            };

                            let type_range = to_single_range(&start, &end);

                            // Add variant to the map, creating a new entry with type range if it doesn't exist
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
    }

    // Check for duplicate fields
    // Check for duplicate tablenames
    //
    for schema in &database.schemas {
        for file in &schema.files {
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
                                    if !context.valid_namespaces.contains(&link.foreign.schema) {
                                        errors.push(Error {
                                            filepath: file.path.clone(),
                                            error_type: ErrorType::LinkToUnknownSchema {
                                                unknown_schema_name: link.foreign.schema.clone(),
                                                known_schemas: context.valid_namespaces.clone(),
                                            },
                                            locations: vec![Location {
                                                contexts: vec![],
                                                primary: to_range(&start, &end),
                                            }],
                                        });
                                    }

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
                                            for foreign_id in &link.foreign.fields {
                                                if !foreign_table
                                                    .record
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
                                                                    .foreign
                                                                    .table
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
                                                    unknown_table: link.foreign.table.clone(),
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
                                                    primary: to_range(
                                                        &link.start_name,
                                                        &link.end_name,
                                                    ),
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
    }

    if errors.len() > 0 {
        return Err(errors);
    } else {
        return Ok(context);
    }
}

fn check_schema_definitions(context: &Context, database: &ast::Database, errors: &mut Vec<Error>) {
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

    let mut session_found = false;

    // Check definitions
    for schema in database.schemas.iter() {
        for file in schema.files.iter() {
            for definition in &file.definitions {
                match definition {
                    ast::Definition::Session(session) => {
                        if session_found {
                            errors.push(Error {
                                filepath: file.path.clone(),
                                error_type: ErrorType::MultipleSessionDeinitions,
                                locations: vec![Location {
                                    contexts: to_range(&session.start, &session.end),
                                    primary: vec![],
                                }],
                            });
                        } else {
                            session_found = true;
                        }
                    }
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
                            if let Some(fields) = &variant.fields {
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
                                                known_types: context
                                                    .types
                                                    .keys()
                                                    .cloned()
                                                    .collect(),
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
}

// Check query

pub fn check_queries<'a>(
    database: &ast::Database,
    query_list: &ast::QueryList,
    context: &mut Context,
) -> Result<HashMap<String, QueryInfo>, Vec<Error>> {
    let mut errors: Vec<Error> = Vec::new();
    let mut all_params: HashMap<String, QueryInfo> = HashMap::new();
    check_schema_definitions(&context, database, &mut errors);

    for query in &query_list.queries {
        match query {
            ast::QueryDef::Query(q) => {
                let query_info = check_query(context, &mut errors, &q);
                all_params.insert(q.name.clone(), query_info);
                continue;
            }
            _ => continue,
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(all_params)
}

#[derive(Debug)]
pub enum ParamInfo {
    Defined {
        // This is the variable name without the $ prefix
        raw_variable_name: String,
        defined_at: Option<Range>,
        type_: Option<String>,
        used_by_top_level_field_alias: HashSet<String>,
        used: bool,
        type_inferred: bool,
        from_session: bool,
        session_name: Option<String>,
    },
    NotDefinedButUsed {
        used_at: Option<Range>,
        type_: Option<String>,
    },
}

pub struct QueryInfo {
    pub variables: HashMap<String, ParamInfo>,
    pub primary_db: String,
    pub attached_dbs: HashSet<String>,
}

struct UsedNamespaces {
    primary: HashSet<String>,
    secondary: HashSet<String>,
}

pub fn check_query(
    context: &mut Context,
    errors: &mut Vec<Error>,
    query: &ast::Query,
) -> QueryInfo {
    // We need to check
    // 1. The field exists on the record in the schema
    //    What type is the field (add to `QueryField`)
    //
    // 2. If params are defined
    //     2.a All defined params are used
    //     2.b Every used param is defined
    //
    let mut param_names: HashMap<String, ParamInfo> = HashMap::new();
    let mut used_namespaces: UsedNamespaces = UsedNamespaces {
        primary: HashSet::new(),
        secondary: HashSet::new(),
    };

    // Check that all param types are known
    // Gather them in param_names so we can calculate usage.
    for param_def in &query.args {
        // formatted name
        let param_name = format!("${}", param_def.name);
        match &param_def.type_ {
            None => {
                param_names.insert(
                    param_name,
                    ParamInfo::Defined {
                        raw_variable_name: param_def.name.clone(),
                        defined_at: to_single_range(&param_def.start_name, &param_def.end_name),
                        type_: None,
                        used: false,
                        used_by_top_level_field_alias: HashSet::new(),
                        type_inferred: false,
                        from_session: false,
                        session_name: None,
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
                    param_name,
                    ParamInfo::Defined {
                        raw_variable_name: param_def.name.clone(),
                        defined_at: to_single_range(&param_def.start_name, &param_def.end_name),
                        type_: Some(type_.clone()),
                        used: false,
                        used_by_top_level_field_alias: HashSet::new(),
                        type_inferred: false,
                        from_session: false,
                        session_name: None,
                    },
                );
            }
        }
    }

    // Add session fields to the param_names collection
    match &context.session {
        None => (),
        Some(session) => {
            for field in &session.fields {
                match field {
                    ast::Field::Column(col) => {
                        param_names.insert(
                            ast::session_field_name(col),
                            ParamInfo::Defined {
                                raw_variable_name: format!("session_{}", col.name),
                                defined_at: None,
                                type_: Some(col.type_.clone()),
                                used: false,
                                used_by_top_level_field_alias: HashSet::new(),
                                type_inferred: false,
                                from_session: true,
                                session_name: Some(col.name.clone()),
                            },
                        );
                    }
                    _ => (),
                }
            }
        }
    }

    // Verify that all fields exist in the schema
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                context.top_level_field_alias = ast::get_aliased_name(query_field);
                match context.tables.get(&query_field.name) {
                    None => errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::UnknownTable {
                            found: query_field.name.clone(),
                            existing: vec![],
                        },
                        locations: vec![Location {
                            contexts: to_range(&query.start, &query.end),
                            primary: to_range(
                                &query_field.start_fieldname,
                                &query_field.end_fieldname,
                            ),
                        }],
                    }),
                    Some(table) => check_table_query(
                        context,
                        errors,
                        &query.operation,
                        None,
                        table,
                        query_field,
                        &mut param_names,
                        &mut used_namespaces,
                    ),
                }
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

    // Check for unused or undefined parameters and add errors accordingly
    for (param_name, param_info) in param_names.iter() {
        match &param_info {
            ParamInfo::Defined {
                defined_at,
                type_,
                used,
                type_inferred,
                from_session,
                ..
            } => {
                if !from_session && *used == false {
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
                } else if !from_session && *type_inferred {
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

    let primary_db = get_primary_db(&used_namespaces).to_string();
    let secondary_dbs = get_secondary_dbs(&used_namespaces, &primary_db);

    QueryInfo {
        variables: param_names,
        primary_db,
        attached_dbs: secondary_dbs,
    }
}

fn get_primary_db(namespaces: &UsedNamespaces) -> &str {
    if namespaces.primary.is_empty() && namespaces.secondary.is_empty() {
        ast::DEFAULT_SCHEMANAME
    } else if !namespaces.primary.is_empty() {
        // This gets an arbitrary value from primary if it has any elements
        namespaces.primary.iter().min().unwrap()
    } else {
        // If primary is empty but secondary isn't, get an arbitrary value from secondary
        namespaces.secondary.iter().min().unwrap()
    }
}

fn get_secondary_dbs(namespaces: &UsedNamespaces, primary_db: &str) -> HashSet<String> {
    let mut secondary = namespaces.secondary.clone();
    secondary.remove(primary_db);
    secondary
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
            let mut is_nullable = false;
            for col in &table.fields {
                if is_known_field {
                    continue;
                }
                match col {
                    ast::Field::Column(column) => {
                        if &column.name == field_name {
                            is_known_field = true;
                            column_type = Some(column.type_.clone());
                            is_nullable = column.nullable;
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
            if !is_known_field {
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
                None => mark_as_used(context, query_val, params),
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
                        is_nullable,
                    );
                }
            }

            // Check if the field exists
            // Get field type
        }
    }
}

fn mark_as_used(
    context: &Context,
    value: &ast::QueryValue,
    params: &mut HashMap<String, ParamInfo>,
) {
    match value {
        ast::QueryValue::Variable((var_range, var)) => {
            let var_name = ast::to_pyre_variable_name(var);
            match params.get_mut(&var_name) {
                None => {
                    params.insert(
                        var_name,
                        ParamInfo::NotDefinedButUsed {
                            used_at: Some(convert_range(var_range)),
                            type_: None,
                        },
                    );
                }
                Some(param_info) => {
                    match param_info {
                        ParamInfo::Defined {
                            ref mut used,
                            ref mut used_by_top_level_field_alias,
                            ..
                        } => {
                            // mark as used
                            *used = true;
                            used_by_top_level_field_alias
                                .insert(context.top_level_field_alias.clone());
                        }
                        ParamInfo::NotDefinedButUsed { used_at, type_ } => (),
                    };
                }
            }
        }
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
    is_nullable: bool,
) {
    match value {
        ast::QueryValue::String((range, _)) => {
            if table_type_string != "String" {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::LiteralTypeMismatch {
                        expecting_type: table_type_string.to_string(),
                        found: "String".to_string(),
                    },
                    locations: vec![Location {
                        contexts: vec![], // to_range(&start, &end),
                        primary: vec![convert_range(range)],
                    }],
                })
            }
        }
        ast::QueryValue::Int((range, _)) => {
            if table_type_string != "Int" {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::LiteralTypeMismatch {
                        expecting_type: table_type_string.to_string(),
                        found: "Int".to_string(),
                    },
                    locations: vec![Location {
                        contexts: vec![], // to_range(&start, &end),
                        primary: vec![convert_range(range)],
                    }],
                })
            }
        }
        ast::QueryValue::Float((range, _)) => {
            if table_type_string != "Float" {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::LiteralTypeMismatch {
                        expecting_type: table_type_string.to_string(),
                        found: "Float".to_string(),
                    },
                    locations: vec![Location {
                        contexts: vec![], // to_range(&start, &end),
                        primary: vec![convert_range(range)],
                    }],
                })
            }
        }
        ast::QueryValue::Bool((range, _)) => {
            if table_type_string != "Bool" {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::LiteralTypeMismatch {
                        expecting_type: table_type_string.to_string(),
                        found: "Bool".to_string(),
                    },
                    locations: vec![Location {
                        contexts: vec![], // to_range(&start, &end),
                        primary: vec![convert_range(range)],
                    }],
                })
            }
        }
        ast::QueryValue::Null(_) => {}
        ast::QueryValue::Fn(func) => {
            let found = context.funcs.get(&func.name);
            match found {
                None => errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::UnknownFunction {
                        found: func.name.clone(),
                        known_functions: context.funcs.keys().cloned().collect(),
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: vec![convert_range(&func.location_fn_name)],
                    }],
                }),
                Some(func_definition) => {
                    if func_definition.arg_types.len() != func.args.len() {
                        errors.push(Error {
                            filepath: context.current_filepath.clone(),
                            error_type: ErrorType::UnknownFunction {
                                found: func.name.clone(),
                                known_functions: context.funcs.keys().cloned().collect(),
                            },
                            locations: vec![Location {
                                contexts: vec![],
                                primary: vec![convert_range(&func.location_fn_name)],
                            }],
                        });
                    } else {
                        for (arg, arg_type) in
                            func.args.iter().zip(func_definition.arg_types.iter())
                        {
                            check_value(
                                context, arg, start, end, errors, params, table_name, arg_type,
                                false,
                            );
                        }
                    }
                }
            }
        }
        ast::QueryValue::Variable((var_range, var)) => {
            let var_name = ast::to_pyre_variable_name(var);

            match params.get_mut(&var_name) {
                None => {
                    params.insert(
                        var_name,
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
                            ref mut used_by_top_level_field_alias,
                            ..
                        } => {
                            // mark as used
                            *used = true;
                            used_by_top_level_field_alias
                                .insert(context.top_level_field_alias.clone());

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
                                                variable_name: var.name.clone(),
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
                        ParamInfo::NotDefinedButUsed { .. } => (),
                    };
                }
            }
        }
        ast::QueryValue::LiteralTypeValue((range, details)) => {
            match context.types.get(table_type_string) {
                Some((type_info, type_)) => {
                    match type_ {
                        Type::OneOf { variants } => {
                            let mut found_variant = false;
                            for variant in variants {
                                if variant.name == details.name {
                                    found_variant = true;
                                    break;
                                }
                            }
                            if !found_variant {
                                errors.push(Error {
                                    filepath: context.current_filepath.clone(),
                                    error_type: ErrorType::LiteralTypeMismatchVariant {
                                        expecting_type: table_type_string.to_string(),
                                        found: details.name.to_string(),
                                        variants: variants.iter().map(|v| v.name.clone()).collect(),
                                    },
                                    locations: vec![Location {
                                        contexts: vec![], // to_range(&start, &end),
                                        primary: vec![convert_range(range)],
                                    }],
                                })
                            }
                        }
                        _ => {
                            errors.push(Error {
                                filepath: context.current_filepath.clone(),
                                error_type: ErrorType::LiteralTypeMismatch {
                                    expecting_type: table_type_string.to_string(),
                                    found: details.name.to_string(),
                                },
                                locations: vec![Location {
                                    contexts: vec![], // to_range(&start, &end),
                                    primary: vec![convert_range(range)],
                                }],
                            })
                        }
                    }
                }
                None => {
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::LiteralTypeMismatch {
                            expecting_type: table_type_string.to_string(),
                            found: details.name.to_string(),
                        },
                        locations: vec![Location {
                            contexts: vec![], // to_range(&start, &end),
                            primary: vec![convert_range(range)],
                        }],
                    })
                }
            }
        }
    }
}

/// Primary schemas/namespaces are the only ones that can accept writes.
/// So, if any field is set, it needs to be on a primary schema.
/// If the operation is a delete, the namespace is always primary
/// Otherwise, it's secondary
fn add_schema(
    context: &Context,
    table: &Table,
    operation: &ast::QueryOperation,
    query: &ast::QueryField,
    field: &ast::QueryField,
    errors: &mut Vec<Error>,
    used_schemas: &mut UsedNamespaces,
) {
    match operation {
        ast::QueryOperation::Delete => {
            insert_primary_schema(
                context,
                table,
                operation,
                query,
                field,
                errors,
                used_schemas,
            );
        }
        ast::QueryOperation::Update | ast::QueryOperation::Insert => {
            if field.set == None {
                used_schemas.secondary.insert(table.schema.to_string());
            } else {
                insert_primary_schema(
                    context,
                    table,
                    operation,
                    query,
                    field,
                    errors,
                    used_schemas,
                );
            }
        }
        ast::QueryOperation::Select => {
            used_schemas.secondary.insert(table.schema.to_string());
        }
    }
}

fn insert_primary_schema(
    context: &Context,
    table: &Table,
    operation: &ast::QueryOperation,
    query: &ast::QueryField,
    field: &ast::QueryField,
    errors: &mut Vec<Error>,
    used_schemas: &mut UsedNamespaces,
) {
    let schema_name = table.schema.to_string();
    // If there's already a primary schema and it's not the current schema
    // Then it's a problem
    if !used_schemas.primary.is_empty() && !used_schemas.primary.contains(&schema_name) {
        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::MultipleSchemaWrites {
                field_table: table.record.name.clone(),
                field_schema: table.schema.to_string(),
                operation: operation.clone(),
                other_schemas: used_schemas.primary.clone().into_iter().collect(),
            },
            locations: vec![Location {
                contexts: to_range(&query.start, &query.end),
                primary: to_range(&field.start_fieldname, &field.end_fieldname),
            }],
        });
    }
    used_schemas.primary.insert(table.schema.to_string());
}

fn check_table_query(
    context: &Context,
    errors: &mut Vec<Error>,
    operation: &ast::QueryOperation,
    through_link: Option<&ast::LinkDetails>,
    table: &Table,
    query: &ast::QueryField,
    params: &mut HashMap<String, ParamInfo>,
    used_namespaces: &mut UsedNamespaces,
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
            ast::ArgField::Lines { .. } => (),
            ast::ArgField::QueryComment { .. } => (),
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
                            &table.record.name,
                            "Int",
                            false,
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
                            &table.record.name,
                            "Int",
                            false,
                        );
                    }
                    ast::Arg::Where(where_args) => {
                        match to_single_range(&arg.start, &arg.end) {
                            Some(range) => wheres.push(range),
                            None => (),
                        }

                        check_where_args(
                            context,
                            &arg.start,
                            &arg.end,
                            &table.record,
                            errors,
                            params,
                            &where_args,
                        );
                    }
                    _ => (),
                }
            }
            ast::ArgField::Field(field) => {
                add_schema(
                    context,
                    &table,
                    operation,
                    query,
                    field,
                    errors,
                    used_namespaces,
                );

                let aliased_name = ast::get_aliased_name(field);

                if queried_fields.get(&aliased_name).is_some() {
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::DuplicateQueryField {
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
                for col in &table.record.fields {
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
                                check_link(
                                    context,
                                    operation,
                                    errors,
                                    &table.record,
                                    link,
                                    field,
                                    params,
                                    used_namespaces,
                                )
                            }
                            ()
                        }
                        _ => (),
                    }
                }
                if !is_known_field {
                    let known_fields = get_column_reference(&table.record.fields);
                    errors.push(Error {
                        filepath: context.current_filepath.clone(),
                        error_type: ErrorType::UnknownField {
                            found: field.name.clone(),
                            record_name: table.record.name.clone(),
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

    // Only one @limit allowed
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

    // Only one @offset is allowed
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

    if (offset_len > 0 || limit_len > 0) && has_nested_selected {
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
            let mut missing_fields = vec![];

            if let Some(link) = through_link {
                for field in &link.foreign.fields {
                    if queried_fields.contains_key(field) {
                        errors.push(Error {
                            filepath: context.current_filepath.clone(),
                            error_type: ErrorType::InsertNestedValueAutomaticallySet {
                                field: field.clone(),
                            },
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&query.start, &query.end),
                            }],
                        });
                        return;
                    }
                }
            }

            for col in ast::collect_columns(&table.record.fields) {
                if ast::is_primary_key(&col)
                    || ast::has_default_value(&col)
                    || through_link.map_or(false, |link| link.foreign.fields.contains(&col.name))
                {
                    // Primary keys, fields with defaults, and  aren't required
                    // (for the moment, we should differentiate between auto-incrementing
                    // and non-auto-incrementing primary keys)
                    continue;
                }

                match queried_fields.get(&col.name) {
                    Some(is_set) => {
                        if !is_set {
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
                    None => {
                        missing_fields.push(col.name.clone());
                    }
                }
            }

            if !missing_fields.is_empty() {
                errors.push(Error {
                    filepath: context.current_filepath.clone(),
                    error_type: ErrorType::InsertMissingColumn {
                        fields: missing_fields.clone(),
                    },
                    locations: vec![Location {
                        contexts: vec![],
                        primary: to_range(&query.start, &query.end),
                    }],
                });
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
                column.nullable,
            );
        }
        None => {}
    }

    match operation {
        ast::QueryOperation::Select => {
            if field.set.is_some() {
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
            if field.set.is_some() {
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
                known_fields.push((link.link_name.clone(), link.foreign.table.clone()))
            }
            _ => (),
        }
    }
    known_fields
}

fn check_link(
    context: &Context,
    operation: &ast::QueryOperation,
    errors: &mut Vec<Error>,
    local_table: &ast::RecordDetails,
    link: &ast::LinkDetails,
    field: &ast::QueryField,
    params: &mut HashMap<String, ParamInfo>,
    used_namespaces: &mut UsedNamespaces,
) {
    match operation {
        ast::QueryOperation::Insert => {
            // Nested inserts are only allowed if the local_id is a primary key
            let primary_key_field_name = ast::get_primary_id_field_name(&local_table.fields);

            match primary_key_field_name {
                None => (),
                Some(primary_key_name) => {
                    let are_primary = link
                        .local_ids
                        .iter()
                        .all(|s: &String| s == &primary_key_name);
                    if !are_primary {
                        errors.push(Error {
                            filepath: context.current_filepath.clone(),
                            error_type: ErrorType::LinksDisallowedInInserts {
                                field: link.link_name.clone(),
                                table_name: local_table.name.clone(),
                                local_ids: link.local_ids.clone(),
                            },
                            locations: vec![Location {
                                contexts: vec![],
                                primary: to_range(&field.start, &field.end),
                            }],
                        });
                    }
                }
            };
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

    if field.fields.is_empty() {
        let known_fields: Vec<(String, String)> = vec![];

        errors.push(Error {
            filepath: context.current_filepath.clone(),
            error_type: ErrorType::LinkSelectionIsEmpty {
                link_name: link.link_name.clone(),
                foreign_table: link.foreign.table.clone(),
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
            .get(&crate::ext::string::decapitalize(&link.foreign.table));
        match table {
            None => errors.push(Error {
                filepath: context.current_filepath.clone(),
                error_type: ErrorType::UnknownTable {
                    found: link.foreign.table.clone(),
                    existing: vec![],
                },
                locations: vec![Location {
                    contexts: vec![],
                    primary: to_range(&field.start, &field.end),
                }],
            }),
            Some(table) => check_table_query(
                context,
                errors,
                operation,
                Some(link),
                table,
                field,
                params,
                used_namespaces,
            ),
        }
    }
}
