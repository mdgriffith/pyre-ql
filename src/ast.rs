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
    pub args: Vec<QueryArgDefinition>,
    pub fields: Vec<QueryField>,
}

#[derive(Debug)]
pub struct QueryArgDefinition {
    pub name: String,
    pub type_: String,
}

#[derive(Debug)]
pub struct QueryField {
    pub name: String,
    pub args: Vec<QueryArg>,
    pub directives: Vec<String>,
    pub fields: Vec<QueryField>,
}

#[derive(Debug)]
pub struct QueryArg {
    pub name: String,
    pub value: QueryValue,
}

#[derive(Debug)]
pub enum QueryValue {
    Variable(String),
    String(String),
    Int(i32),
    Float(f32),
    Bool(bool),
    Null,
}
