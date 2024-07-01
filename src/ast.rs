use crate::ext::string;
use nom_locate;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct Schema {
    pub definitions: Vec<Definition>,
}

pub fn empty_schema() -> Schema {
    Schema {
        definitions: Vec::new(),
    }
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
    Record {
        name: String,
        fields: Vec<Field>,
        start: Option<Location>,
        end: Option<Location>,

        start_name: Option<Location>,
        end_name: Option<Location>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RecordDetails {
    pub name: String,
    pub fields: Vec<Field>,

    pub start: Option<Location>,
    pub end: Option<Location>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Variant {
    pub name: String,
    pub data: Option<Vec<Field>>,
    pub start: Option<Location>,
    pub end: Option<Location>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum Field {
    Column(Column),
    ColumnLines { count: usize },
    ColumnComment { text: String },
    FieldDirective(FieldDirective),
}

pub fn is_link(field: &Field) -> bool {
    match field {
        Field::FieldDirective(FieldDirective::Link(_)) => true,
        _ => false,
    }
}

pub fn is_primary_key(col: &Column) -> bool {
    col.directives
        .iter()
        .any(|d| *d == ColumnDirective::PrimaryKey)
}

pub fn get_tablename(name: &str, fields: &Vec<Field>) -> String {
    for field in fields.iter() {
        match field {
            Field::FieldDirective(FieldDirective::TableName((range, name))) => {
                return name.to_string()
            }
            _ => {}
        }
    }
    name.to_string()
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

pub fn is_field_directive(field: &Field) -> bool {
    match field {
        Field::FieldDirective(_) => true,
        _ => false,
    }
}

pub fn is_column_comment(field: &Field) -> bool {
    match field {
        Field::ColumnComment { .. } => true,
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
    a.foreign_tablename == b.foreign_tablename
        && a.local_ids == b.local_ids
        && a.foreign_ids == b.foreign_ids
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum FieldDirective {
    TableName((Range, String)),
    Link(LinkDetails),
}

pub fn link_identity(local_table: &str, link: &LinkDetails) -> String {
    format!(
        "{}_{}_{}_{}_fk",
        local_table,
        &link.local_ids.join("_"),
        link.foreign_tablename,
        &link.foreign_ids.join("_"),
    )
}

pub fn to_reciprocal(local_table: &str, link: &LinkDetails) -> LinkDetails {
    LinkDetails {
        link_name: string::pluralize(&string::decapitalize(local_table)),
        local_ids: link.foreign_ids.clone(),
        foreign_tablename: local_table.to_string(),
        foreign_ids: link.local_ids.clone(),
        start_name: None,
        end_name: None,
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LinkDetails {
    pub link_name: String,
    pub local_ids: Vec<String>,

    pub foreign_tablename: String,
    pub foreign_ids: Vec<String>,

    pub start_name: Option<Location>,
    pub end_name: Option<Location>,
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

pub fn column_order(a: &Field, b: &Field) -> std::cmp::Ordering {
    match (a, b) {
        (Field::FieldDirective(_), Field::FieldDirective(_)) => std::cmp::Ordering::Equal,
        (Field::ColumnComment { .. }, Field::FieldDirective(_)) => std::cmp::Ordering::Equal,
        (Field::FieldDirective(_), Field::ColumnComment { .. }) => std::cmp::Ordering::Equal,
        (Field::FieldDirective(_), _) => std::cmp::Ordering::Less,
        (_, Field::FieldDirective(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
pub struct Location {
    pub offset: usize,
    pub line: u32,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum ColumnDirective {
    PrimaryKey,
    Unique,
    Default(DefaultValue),
    // Check(String),
}

// CURRENT_TIME, CURRENT_DATE or CURRENT_TIMESTAMP
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum SerializationType {
    Integer,
    Real,
    Text,
    BlobWithSchema(String),
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
    pub operation: QueryOperation,
    pub name: String,
    pub args: Vec<QueryParamDefinition>,
    pub fields: Vec<QueryField>,

    pub start: Option<Location>,
    pub end: Option<Location>,
}

#[derive(Debug, Clone)]
pub enum QueryOperation {
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
pub struct QueryParamDefinition {
    pub name: String,
    pub type_: String,
    pub start_name: Option<Location>,
    pub end_name: Option<Location>,

    pub start_type: Option<Location>,
    pub end_type: Option<Location>,
}

// The "Select Alias" is the value that is in the JSON payload
pub fn get_select_alias(
    table_alias: &str,
    table_field: &Field,
    query_field: &QueryField,
) -> String {
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
    Line { count: usize },
}

#[derive(Debug, Clone)]
pub struct LocatedArg {
    pub arg: Arg,
    pub start: Option<Location>,
    pub end: Option<Location>,
}

pub fn is_query_field_arg(field: &ArgField) -> bool {
    match field {
        ArgField::Arg(_) => true,
        _ => false,
    }
}

pub fn query_field_order(a: &ArgField, b: &ArgField) -> std::cmp::Ordering {
    match (a, b) {
        (ArgField::Arg(_), ArgField::Arg(_)) => std::cmp::Ordering::Equal,
        (ArgField::Arg(_), _) => std::cmp::Ordering::Less,
        (_, ArgField::Arg(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
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

pub fn collect_query_args(fields: &Vec<ArgField>) -> Vec<Arg> {
    let mut args = Vec::new();
    for field in fields {
        if let ArgField::Arg(arg) = field {
            args.push(arg.arg.clone());
        }
    }
    args
}

#[derive(Debug, Clone)]
pub enum Arg {
    Limit(QueryValue),
    Offset(QueryValue),
    OrderBy(Direction, String),
    Where(WhereArg),
}

pub fn collect_where_args<'a>(args: &'a Vec<Arg>) -> Vec<&'a WhereArg> {
    let mut wheres = Vec::new();
    for arg in args {
        match arg {
            Arg::Where(wher) => wheres.push(wher),
            _ => {}
        }
    }
    wheres
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

#[derive(Debug, Clone)]
pub enum WhereArg {
    Column(String, Operator, QueryValue),
    And(Vec<WhereArg>),
    Or(Vec<WhereArg>),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum QueryValue {
    Variable((Range, String)),
    String((Range, String)),
    Int((Range, i32)),
    Float((Range, f32)),
    Bool((Range, bool)),
    Null(Range),
}

#[derive(Debug, Clone)]
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
