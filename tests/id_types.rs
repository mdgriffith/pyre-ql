use pyre::ast;
use pyre::error::ErrorType;
use pyre::parser;
use pyre::typecheck;

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

#[test]
fn test_foreign_key_to_unknown_table_error() {
    let schema_source = r#"
record Post {
    @public
    id Id.Int @id
    authorId NonExistent.id
    title String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let result = typecheck::check_schema(&database);

    match result {
        Ok(_) => {
            panic!("Expected typechecking to fail due to foreign key referencing unknown table");
        }
        Err(errors) => {
            let fk_error = errors.iter().find(|e| {
                matches!(
                    &e.error_type,
                    ErrorType::ForeignKeyToUnknownTable {
                        field_name,
                        referenced_table,
                        ..
                    }
                    if field_name == "authorId" && referenced_table == "NonExistent"
                )
            });

            assert!(
                fk_error.is_some(),
                "Expected ForeignKeyToUnknownTable error for authorId referencing NonExistent.id, got: {:?}",
                errors
            );

            // Verify error has location info
            let error = fk_error.unwrap();
            assert!(
                !error.locations.is_empty(),
                "Error should have at least one location"
            );
        }
    }
}

#[test]
fn test_foreign_key_to_unknown_field_error() {
    let schema_source = r#"
record User {
    @public
    id Id.Int @id
    name String
}

record Post {
    @public
    id Id.Int @id
    authorId User.nonexistent
    title String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let result = typecheck::check_schema(&database);

    match result {
        Ok(_) => {
            panic!("Expected typechecking to fail due to foreign key referencing unknown field");
        }
        Err(errors) => {
            let fk_error = errors.iter().find(|e| {
                matches!(
                    &e.error_type,
                    ErrorType::ForeignKeyToUnknownField {
                        field_name,
                        referenced_table,
                        referenced_field,
                        ..
                    }
                    if field_name == "authorId" && referenced_table == "User" && referenced_field == "nonexistent"
                )
            });

            assert!(
                fk_error.is_some(),
                "Expected ForeignKeyToUnknownField error for authorId referencing User.nonexistent, got: {:?}",
                errors
            );

            // Verify existing_fields are provided for helpful error messages
            if let Some(error) = fk_error {
                if let ErrorType::ForeignKeyToUnknownField {
                    existing_fields, ..
                } = &error.error_type
                {
                    assert!(
                        existing_fields.contains(&"id".to_string()),
                        "Error should list 'id' as an existing field"
                    );
                    assert!(
                        existing_fields.contains(&"name".to_string()),
                        "Error should list 'name' as an existing field"
                    );
                }
            }
        }
    }
}

#[test]
fn test_foreign_key_to_non_id_field_error() {
    let schema_source = r#"
record User {
    @public
    id Id.Int @id
    name String
}

record Post {
    @public
    id Id.Int @id
    authorId User.name
    title String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let result = typecheck::check_schema(&database);

    match result {
        Ok(_) => {
            panic!("Expected typechecking to fail due to foreign key referencing non-ID field");
        }
        Err(errors) => {
            let fk_error = errors.iter().find(|e| {
                matches!(
                    &e.error_type,
                    ErrorType::ForeignKeyToNonIdField {
                        field_name,
                        referenced_table,
                        referenced_field,
                        referenced_field_type,
                    }
                    if field_name == "authorId"
                        && referenced_table == "User"
                        && referenced_field == "name"
                        && referenced_field_type == "String"
                )
            });

            assert!(
                fk_error.is_some(),
                "Expected ForeignKeyToNonIdField error for authorId referencing User.name (String), got: {:?}",
                errors
            );
        }
    }
}

#[test]
fn test_valid_foreign_key_passes_validation() {
    let schema_source = r#"
record User {
    @public
    id Id.Int @id
    name String
}

record Post {
    @public
    id Id.Int @id
    authorId User.id
    title String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let result = typecheck::check_schema(&database);

    assert!(
        result.is_ok(),
        "Valid foreign key reference should pass validation, but got: {:?}",
        result.err()
    );
}

#[test]
fn test_foreign_key_with_id_uuid_passes_validation() {
    let schema_source = r#"
record User {
    @public
    id Id.Uuid @id
    name String
}

record Post {
    @public
    id Id.Int @id
    authorId User.id
    title String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let result = typecheck::check_schema(&database);

    assert!(
        result.is_ok(),
        "Foreign key reference to Id.Uuid should pass validation, but got: {:?}",
        result.err()
    );
}
