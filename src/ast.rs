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
    },
    Record {
        name: String,
        fields: Vec<Field>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordDetails {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub data: Option<Vec<Field>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Column(Column),
    ColumnLines { count: usize },
    ColumnComment { text: String },
    FieldDirective(FieldDirective),
}

pub fn get_tablename(name: &str, fields: &Vec<Field>) -> String {
    for field in fields.iter() {
        match field {
            Field::FieldDirective(FieldDirective::TableName(name)) => return name.to_string(),
            _ => {}
        }
    }
    name.to_string()
}

pub fn is_field_directive(field: &Field) -> bool {
    match field {
        Field::FieldDirective(_) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDirective {
    TableName(String),
    Link(LinkDetails),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkDetails {
    pub link_name: String,
    pub local_ids: Vec<String>,

    pub foreign_tablename: String,
    pub foreign_ids: Vec<String>,
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

pub fn column_order(a: &Field, b: &Field) -> std::cmp::Ordering {
    match (a, b) {
        (Field::FieldDirective(_), Field::FieldDirective(_)) => std::cmp::Ordering::Equal,
        (Field::FieldDirective(_), _) => std::cmp::Ordering::Less,
        (_, Field::FieldDirective(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub type_: String,
    pub serialization_type: SerializationType,
    pub nullable: bool,
    pub directives: Vec<ColumnDirective>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnDirective {
    PrimaryKey,
    Unique,
    // Default(String),
    // Check(String),
    // ForeignKey(String),
}

// https://sqlite.org/datatype3.html
// NULL. The value is a NULL value.
// INTEGER. The value is a signed integer, stored in 0, 1, 2, 3, 4, 6, or 8 bytes depending on the magnitude of the value.
// REAL. The value is a floating point value, stored as an 8-byte IEEE floating point number.
// TEXT. The value is a text string, stored using the database encoding (UTF-8, UTF-16BE or UTF-16LE).
// BLOB. The value is a blob of data, stored exactly as it was input.
#[derive(Debug, Clone, PartialEq)]
pub enum SerializationType {
    Integer,
    Real,
    Text,
    BlobWithSchema(String),
}

// Queries
//
#[derive(Debug)]
pub struct QueryList {
    pub queries: Vec<QueryDef>,
}

#[derive(Debug)]
pub enum QueryDef {
    Query(Query),
    QueryComment { text: String },
    QueryLines { count: usize },
}

#[derive(Debug)]
pub struct Query {
    pub name: String,
    pub args: Vec<QueryParamDefinition>,
    pub fields: Vec<QueryField>,
}

#[derive(Debug)]
pub struct QueryParamDefinition {
    pub name: String,
    pub type_: String,
}

#[derive(Debug)]
pub struct QueryField {
    pub name: String,
    pub params: Vec<QueryParam>,
    pub directives: Vec<String>,
    pub fields: Vec<QueryField>,
}

#[derive(Debug)]
pub struct QueryParam {
    pub name: String,
    pub operator: Operator,
    pub value: QueryValue,
}

#[derive(Debug, Clone)]
pub enum QueryValue {
    Variable(String),
    String(String),
    Int(i32),
    Float(f32),
    Bool(bool),
    Null,
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
