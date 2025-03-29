use crate::ast::{Column, ColumnDirective, Definition, Field, SchemaFile, SerializationType};
use crate::db::introspect::{ColumnInfo, Introspection};

pub fn to_schema(introspection: &Introspection) -> SchemaFile {
    let mut definitions = Vec::new();

    for table in &introspection.tables {
        let mut fields = Vec::new();

        // Convert columns to fields
        for column in &table.columns {
            fields.push(Field::Column(column_info_to_column(column)));
        }

        // Add the record definition
        definitions.push(Definition::Record {
            name: table.name.clone(),
            fields,
            start: None,
            end: None,
            start_name: None,
            end_name: None,
        });
    }

    SchemaFile {
        path: String::from("schema.sql"), // Default path
        definitions,
    }
}

fn column_info_to_column(info: &ColumnInfo) -> Column {
    let mut directives = Vec::new();

    // Handle primary key
    if info.pk {
        directives.push(ColumnDirective::PrimaryKey);
    }

    // Handle not null constraint
    let nullable = !info.notnull;

    // Basic type mapping from SQLite to our SerializationType
    let serialization_type = match info.column_type.to_lowercase().as_str() {
        "integer" => SerializationType::Concrete(crate::ast::ConcreteSerializationType::Integer),
        "real" => SerializationType::Concrete(crate::ast::ConcreteSerializationType::Real),
        "text" => SerializationType::Concrete(crate::ast::ConcreteSerializationType::Text),
        "blob" => SerializationType::Concrete(crate::ast::ConcreteSerializationType::Blob),
        _ => SerializationType::Concrete(crate::ast::ConcreteSerializationType::Text), // Default to text
    };

    Column {
        name: info.name.clone(),
        type_: info.column_type.clone(),
        serialization_type,
        nullable,
        directives,
        start: None,
        end: None,
        start_name: None,
        end_name: None,
        start_typename: None,
        end_typename: None,
    }
}
