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
pub struct Variant {
    pub name: String,
    pub data: Option<Vec<Field>>,
}

#[derive(Debug)]
pub struct RecordDetails {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
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
