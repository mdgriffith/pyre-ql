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
