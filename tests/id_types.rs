use pyre::ast;
use pyre::parser;

#[test]
fn test_id_int_parsing() {
    let schema_source = r#"
record User {
    id Id.Int @id
    name String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Id.Int should parse successfully");

    // Check that the brand was set
    let record = schema.files[0]
        .definitions
        .iter()
        .find_map(|d| match d {
            ast::Definition::Record { name, fields, .. } if name == "User" => Some(fields),
            _ => None,
        })
        .expect("Should find User record");

    let id_field = record
        .iter()
        .find_map(|f| match f {
            ast::Field::Column(col) if col.name == "id" => Some(col),
            _ => None,
        })
        .expect("Should find id field");

    assert_eq!(
        id_field.type_,
        ast::ColumnType::IdInt {
            table: String::new()
        }
    );
}

#[test]
fn test_id_uuid_parsing() {
    let schema_source = r#"
record Invite {
    id Id.Uuid @id
    code String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(result.is_ok(), "Id.Uuid should parse successfully");

    let record = schema.files[0]
        .definitions
        .iter()
        .find_map(|d| match d {
            ast::Definition::Record { name, fields, .. } if name == "Invite" => Some(fields),
            _ => None,
        })
        .expect("Should find Invite record");

    let id_field = record
        .iter()
        .find_map(|f| match f {
            ast::Field::Column(col) if col.name == "id" => Some(col),
            _ => None,
        })
        .expect("Should find id field");

    assert_eq!(
        id_field.type_,
        ast::ColumnType::IdUuid {
            table: String::new()
        }
    );
}

#[test]
fn test_foreign_key_field_reference_parsing() {
    let schema_source = r#"
record User {
    id Id.Int @id
    name String
}

record Post {
    id Id.Int @id
    authorId User.id
    title String
}
"#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Foreign key reference should parse successfully"
    );

    let post_record = schema.files[0]
        .definitions
        .iter()
        .find_map(|d| match d {
            ast::Definition::Record { name, fields, .. } if name == "Post" => Some(fields),
            _ => None,
        })
        .expect("Should find Post record");

    let author_id_field = post_record
        .iter()
        .find_map(|f| match f {
            ast::Field::Column(col) if col.name == "authorId" => Some(col),
            _ => None,
        })
        .expect("Should find authorId field");

    assert_eq!(
        author_id_field.type_,
        ast::ColumnType::ForeignKey {
            table: "User".to_string(),
            field: "id".to_string()
        }
    );
}
