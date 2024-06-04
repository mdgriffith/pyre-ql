#[derive(Debug)]
pub struct Schema {
    pub definitions: Vec<Definition>,
}

#[repr(u8)]
#[derive(Debug)]
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

#[derive(Debug)]
pub struct Variant {
    pub name: String,
    pub data: Option<Vec<Field>>,
}

#[derive(Debug)]
pub struct RecordDetails {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub type_: String,
    pub directives: Vec<String>,
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
