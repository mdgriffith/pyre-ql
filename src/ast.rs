use crate::ext::string;
use serde::{Deserialize, Serialize};

pub mod diff;

#[derive(Debug)]
pub struct Database {
    pub schemas: Vec<Schema>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub namespace: String,
    pub session: Option<SessionDetails>,
    pub files: Vec<SchemaFile>,
}

pub const DEFAULT_SCHEMANAME: &str = "_default";

impl Default for Schema {
    fn default() -> Self {
        Schema {
            namespace: DEFAULT_SCHEMANAME.to_string(),
            session: None,
            files: Vec::new(),
        }
    }
}

pub fn default_session_details() -> SessionDetails {
    SessionDetails {
        fields: Vec::new(),
        start: None,
        end: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDetails {
    pub fields: Vec<Field>,

    pub start: Option<Location>,
    pub end: Option<Location>,
}

pub fn is_empty_schema(schema: &Schema) -> bool {
    for file in schema.files.iter() {
        if file.definitions.len() > 0 {
            return true;
        }
    }
    return false;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaFile {
    pub path: String,
    pub definitions: Vec<Definition>,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum Definition {
    Lines {
        count: usize,
    },
    Comment {
        text: String,
    },
    Tagged {
        name: String,
        variants: Vec<Variant>,
        start: Option<Location>,
        end: Option<Location>,
    },
    Session(SessionDetails),
    Record {
        name: String,
        fields: Vec<Field>,
        start: Option<Location>,
        end: Option<Location>,

        start_name: Option<Location>,
        end_name: Option<Location>,
    },
}
pub fn get_permissions(record: &RecordDetails, operation: &QueryOperation) -> Option<WhereArg> {
    for field in &record.fields {
        if let Field::FieldDirective(directive) = field {
            match directive {
                FieldDirective::Permissions(perm) => match perm {
                    PermissionDetails::Star(where_arg) => return Some(where_arg.clone()),
                    PermissionDetails::OnOperation(ops) => {
                        let mut matching_wheres = Vec::new();
                        for op in ops {
                            for op_type in &op.operations {
                                if *op_type == *operation {
                                    matching_wheres.push(op.where_.clone());
                                }
                            }
                        }

                        if matching_wheres.is_empty() {
                            return None;
                        } else if matching_wheres.len() == 1 {
                            return Some(matching_wheres.remove(0));
                        } else {
                            return Some(WhereArg::And(matching_wheres));
                        }
                    }
                },
                _ => {}
            }
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordDetails {
    pub name: String,
    pub fields: Vec<Field>,

    pub start: Option<Location>,
    pub end: Option<Location>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Option<Vec<Field>>,
    pub start: Option<Location>,
    pub end: Option<Location>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
}

pub fn to_variant(name: &str) -> Variant {
    Variant {
        name: name.to_string(),
        fields: None,
        start: None,
        end: None,
        start_name: None,
        end_name: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Column(Column),
    ColumnLines { count: usize },
    ColumnComment { text: String },
    FieldDirective(FieldDirective),
}

pub fn has_default_value(col: &Column) -> bool {
    col.directives.iter().any(|d| match d {
        ColumnDirective::Default { .. } => true,
        _ => false,
    })
}

pub fn get_primary_id_field_name(fields: &Vec<Field>) -> Option<String> {
    for field in fields.iter() {
        match field {
            Field::Column(col) => {
                if is_primary_key(col) {
                    return Some(col.name.clone());
                }
            }
            _ => {}
        }
    }
    None
}

pub fn is_field_primary_key(field_names: &Vec<String>, field: &Vec<Field>) -> bool {
    field.iter().any(|f| match f {
        Field::Column(col) => is_primary_key(col) && field_names.contains(&col.name),
        _ => false,
    })
}

pub fn is_primary_key(col: &Column) -> bool {
    col.directives
        .iter()
        .any(|d| *d == ColumnDirective::PrimaryKey)
}

pub fn to_watched_operations(record: &RecordDetails) -> Vec<QueryOperation> {
    let mut ops = Vec::new();
    for field in record.fields.iter() {
        match field {
            Field::FieldDirective(FieldDirective::Watched(details)) => {
                if details.selects {
                    ops.push(QueryOperation::Select);
                }
                if details.inserts {
                    ops.push(QueryOperation::Insert);
                }
                if details.updates {
                    ops.push(QueryOperation::Update);
                }
                if details.deletes {
                    ops.push(QueryOperation::Delete);
                }
            }
            _ => {}
        }
    }
    ops
}

pub fn get_tablename(record_name: &str, fields: &Vec<Field>) -> String {
    for field in fields.iter() {
        match field {
            Field::FieldDirective(FieldDirective::TableName((_, name))) => return name.to_string(),
            _ => {}
        }
    }

    string::pluralize(&string::decapitalize(record_name))
}

pub fn has_fieldname(field: &Field, desired_name: &str) -> bool {
    match field {
        Field::Column(Column { name, .. }) => name == desired_name,
        _ => false,
    }
}

pub fn has_field_or_linkname(field: &Field, desired_name: &str) -> bool {
    match field {
        Field::Column(Column { name, .. }) => name == desired_name,
        Field::FieldDirective(FieldDirective::Link(link)) => link.link_name == desired_name,
        _ => false,
    }
}

pub fn has_link_named(field: &Field, desired_name: &str) -> bool {
    match field {
        Field::FieldDirective(FieldDirective::Link(link)) => link.link_name == desired_name,
        _ => false,
    }
}

pub fn is_column(field: &Field) -> bool {
    match field {
        Field::Column { .. } => true,
        _ => false,
    }
}

pub fn is_column_space(field: &Field) -> bool {
    match field {
        Field::ColumnLines { .. } => true,
        _ => false,
    }
}

pub fn link_equivalent(a: &LinkDetails, b: &LinkDetails) -> bool {
    a.local_ids == b.local_ids && a.foreign == b.foreign
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDirective {
    Watched(WatchedDetails),
    TableName((Range, String)),
    Link(LinkDetails),
    Permissions(PermissionDetails),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDetails {
    Star(WhereArg),
    OnOperation(Vec<PermissionOnOperation>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionOnOperation {
    pub operations: Vec<QueryOperation>,
    pub where_: WhereArg,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionOperations {
    pub select: Option<WhereArg>,
    pub insert: Option<WhereArg>,
    pub update: Option<WhereArg>,
    pub delete: Option<WhereArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchedDetails {
    pub selects: bool,
    pub inserts: bool,
    pub updates: bool,
    pub deletes: bool,
}

pub fn link_identity(local_table: &str, link: &LinkDetails) -> String {
    format!(
        "{}_{}_{}_{}_fk",
        local_table,
        &link.local_ids.join("_"),
        link.foreign.table,
        &link.foreign.fields.join("-"),
    )
}

pub fn linked_to_unique_field(link: &LinkDetails) -> bool {
    // Fallback: check if any field is named "id" (primary keys are always unique)
    // For proper uniqueness checking, use linked_to_unique_field_with_record
    link.foreign.fields.iter().any(|f| f == "id")
}

/// Check if a link points to unique fields by examining the foreign table's schema.
/// This properly checks for UNIQUE constraints and PRIMARY KEY constraints.
pub fn linked_to_unique_field_with_record(
    link: &LinkDetails,
    foreign_record: &RecordDetails,
) -> bool {
    // If linking to a single field, check if that field has UNIQUE or PRIMARY KEY constraint
    if link.foreign.fields.len() == 1 {
        let field_name = &link.foreign.fields[0];
        for field in &foreign_record.fields {
            match field {
                Field::Column(column) => {
                    if column.name == *field_name {
                        // Check if this column has UNIQUE or PRIMARY KEY constraint
                        return column.directives.iter().any(|d| {
                            matches!(d, ColumnDirective::Unique | ColumnDirective::PrimaryKey)
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // For multi-field links, we'd need to check for composite UNIQUE constraints
    // For now, fall back to checking if all fields are "id" (which is a common pattern)
    // TODO: Support composite UNIQUE constraints
    link.foreign.fields.iter().all(|f| f == "id")
}

pub fn to_reciprocal(local_namespace: &str, local_table: &str, link: &LinkDetails) -> LinkDetails {
    LinkDetails {
        link_name: string::pluralize(&string::decapitalize(local_table)),
        local_ids: link.foreign.fields.clone(),

        foreign: Qualified {
            schema: local_namespace.to_string(),
            table: local_table.to_string(),
            fields: link.local_ids.clone(),
        },
        start_name: None,
        end_name: None,
    }
}

pub fn get_foreign_tablename(schema: &Schema, link: &LinkDetails) -> String {
    for file in schema.files.iter() {
        for definition in file.definitions.iter() {
            match definition {
                Definition::Record { name, fields, .. } => {
                    if name == &link.foreign.table {
                        return get_tablename(&link.foreign.table, fields);
                    }
                }
                _ => {}
            }
        }
    }
    link.foreign.table.clone()
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkDetails {
    pub link_name: String,
    pub local_ids: Vec<String>,

    pub foreign: Qualified,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Qualified {
    pub schema: String,
    pub table: String,
    pub fields: Vec<String>,
}

pub fn collect_columns(fields: &Vec<Field>) -> Vec<Column> {
    let mut columns = Vec::new();
    for field in fields {
        match field {
            Field::Column(column) => columns.push(column.clone()),
            _ => {}
        }
    }
    columns
}

pub fn collect_links(fields: &Vec<Field>) -> Vec<LinkDetails> {
    let mut links = Vec::new();
    for field in fields {
        match field {
            Field::FieldDirective(FieldDirective::Link(link)) => links.push(link.clone()),
            _ => {}
        }
    }
    links
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub type_: String,
    pub serialization_type: SerializationType,
    pub nullable: bool,
    pub directives: Vec<ColumnDirective>,

    pub start: Option<Location>,
    pub end: Option<Location>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,

    pub start_typename: Option<Location>,
    pub end_typename: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub offset: usize,
    pub line: u32,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: Location,
    pub end: Location,
}

pub fn empty_range() -> Range {
    Range {
        start: Location {
            offset: 0,
            line: 0,
            column: 0,
        },
        end: Location {
            offset: 0,
            line: 0,
            column: 0,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnDirective {
    PrimaryKey,
    Unique,
    Default { id: String, value: DefaultValue },
    // Check(String),
}

// CURRENT_TIME, CURRENT_DATE or CURRENT_TIMESTAMP
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DefaultValue {
    Now,
    Value(QueryValue),
}

// https://sqlite.org/datatype3.html
// NULL. The value is a NULL value.
// INTEGER. The value is a signed integer, stored in 0, 1, 2, 3, 4, 6, or 8 bytes depending on the magnitude of the value.
// REAL. The value is a floating point value, stored as an 8-byte IEEE floating point number.
// TEXT. The value is a text string, stored using the database encoding (UTF-8, UTF-16BE or UTF-16LE).
// BLOB. The value is a blob of data, stored exactly as it was input.
#[derive(Debug, Clone, PartialEq)]
pub enum SerializationType {
    Concrete(ConcreteSerializationType),
    FromType(String), // defined as another named type.
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConcreteSerializationType {
    Integer,
    Real,
    Text,
    Blob,
    Date,     // stored as a string
    DateTime, // stored as unix epoch integer
    VectorBlob {
        vector_type: VectorType,
        dimensionality: u8,
    },
    JsonB, // This is a blob, but we know it's valid json
}

impl ConcreteSerializationType {
    pub fn to_sql_type(&self) -> String {
        match self {
            ConcreteSerializationType::Integer => "INTEGER".to_string(),
            ConcreteSerializationType::Real => "REAL".to_string(),
            ConcreteSerializationType::Text => "TEXT".to_string(),
            ConcreteSerializationType::Blob => "BLOB".to_string(),
            ConcreteSerializationType::Date => "TEXT".to_string(), // Dates stored as strings
            ConcreteSerializationType::DateTime => "INTEGER".to_string(), // DateTime as unix epoch
            ConcreteSerializationType::VectorBlob { .. } => "BLOB".to_string(),
            ConcreteSerializationType::JsonB => "BLOB".to_string(),
        }
    }
}

// Taken from:
// https://docs.turso.tech/features/ai-and-embeddings#types
#[derive(Debug, Clone, PartialEq)]
pub enum VectorType {
    Float64,
    Float32,
    Float16,
    BFloat16,
    Float8,
    Float1,
}

// Queries
//
#[derive(Debug, Clone)]
pub struct QueryList {
    pub queries: Vec<QueryDef>,
}

#[derive(Debug, Clone)]
pub enum QueryDef {
    Query(Query),
    QueryComment { text: String },
    QueryLines { count: usize },
}

#[derive(Debug, Clone)]
pub struct Query {
    pub interface_hash: String,
    pub full_hash: String,

    pub operation: QueryOperation,
    pub name: String,
    pub args: Vec<QueryParamDefinition>,
    pub fields: Vec<TopLevelQueryField>,

    pub start: Option<Location>,
    pub end: Option<Location>,
}

// This is the first layer of fields in a query
//
#[derive(Debug, Clone)]
pub enum TopLevelQueryField {
    Field(QueryField),
    Lines { count: usize },
    Comment { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryOperation {
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
pub struct QueryParamDefinition {
    pub name: String,
    pub type_: Option<String>,
    pub start_name: Option<Location>,
    pub end_name: Option<Location>,

    pub start_type: Option<Location>,
    pub end_type: Option<Location>,
}

pub fn to_typescript_type(type_: &str) -> String {
    match type_ {
        "String" => "\"string\"".to_string(),
        "Int" => "\"number\"".to_string(),
        "Bool" => "\"bool\"".to_string(),
        "DateTime" => "\"date\"".to_string(),
        _ => type_.to_string(),
    }
}

// The "Select Alias" is the value that is in the JSON payload
// It's used for rectangle returns though.
// For the form is qualified like {table_alias}__{field_alias}
// This isn't currently used for nested returns (which is what is used)
pub fn get_select_alias(table_alias: &str, query_field: &QueryField) -> String {
    let field_alias = get_aliased_name(query_field);

    format!("{}__{}", table_alias, field_alias)
}

//
pub fn get_aliased_name(field: &QueryField) -> String {
    match &field.alias {
        Some(alias) => alias.to_string(),
        None => field.name.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct QueryField {
    pub name: String,
    pub alias: Option<String>,
    pub set: Option<QueryValue>,
    pub directives: Vec<String>,
    pub fields: Vec<ArgField>,

    pub start_fieldname: Option<Location>,
    pub end_fieldname: Option<Location>,

    pub start: Option<Location>,
    pub end: Option<Location>,
}

#[derive(Debug, Clone)]
pub enum ArgField {
    Field(QueryField),
    Arg(LocatedArg),
    Lines { count: usize },
    QueryComment { text: String },
}

#[derive(Debug, Clone)]
pub struct LocatedArg {
    pub arg: Arg,
    pub start: Option<Location>,
    pub end: Option<Location>,
}

pub fn collect_query_fields(fields: &Vec<ArgField>) -> Vec<&QueryField> {
    let mut args = Vec::new();
    for field in fields {
        match field {
            ArgField::Field(arg) => args.push(arg),
            _ => {}
        }
    }
    args
}

pub fn collect_wheres(fields: &Vec<ArgField>) -> Vec<WhereArg> {
    let mut wheres = Vec::new();
    for field in fields {
        if let ArgField::Arg(arg) = field {
            if let Arg::Where(where_arg) = &arg.arg {
                wheres.push(where_arg.clone());
            }
        }
    }
    wheres
}

#[derive(Debug, Clone)]
pub enum Arg {
    Limit(QueryValue),
    Offset(QueryValue),
    OrderBy(Direction, String),
    Where(WhereArg),
}

#[derive(Debug, Clone)]
pub enum Direction {
    Asc,
    Desc,
}

pub fn direction_to_string(direction: &Direction) -> String {
    match direction {
        Direction::Asc => "asc".to_string(),
        Direction::Desc => "desc".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WhereArg {
    Column(bool, String, Operator, QueryValue), // bool indicates if column is from session, String is field name without Session. prefix
    And(Vec<WhereArg>),
    Or(Vec<WhereArg>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryValue {
    Fn(FnDetails),

    LiteralTypeValue((Range, LiteralTypeValueDetails)),
    Variable((Range, VariableDetails)),
    String((Range, String)),
    Int((Range, i32)),
    Float((Range, f32)),
    Bool((Range, bool)),
    Null(Range),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiteralTypeValueDetails {
    pub name: String,
    // Union variant field assignments (e.g., for Create { name = $name, description = $description })
    pub fields: Option<Vec<(String, QueryValue)>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FnDetails {
    pub name: String,
    pub args: Vec<QueryValue>,

    pub location: Range,
    pub location_fn_name: Range,
    pub location_arg: Range,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDetails {
    pub name: String,
    pub session_field: Option<String>,
}

pub fn to_pyre_variable_name(var: &VariableDetails) -> String {
    match &var.session_field {
        Some(session_field) => format!("Session.{}", session_field),
        None => format!("${}", var.name),
    }
}

pub fn session_field_name(col: &Column) -> String {
    // field.name.to_string()
    format!("Session.{}", col.name)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    In,
    NotIn,
    Like,
    NotLike,
}
