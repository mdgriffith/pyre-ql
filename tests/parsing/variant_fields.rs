use pyre::ast;
use pyre::parser;

#[test]
fn test_single_field_on_same_line() {
    let schema_source = r#"
type DevActionType
    = Navigate { to String }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Single field on same line should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the type was parsed correctly
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];

    // Find the DevActionType definition
    let tagged_def = file.definitions.iter().find_map(|d| {
        if let ast::Definition::Tagged { name, variants, .. } = d {
            if name == "DevActionType" {
                return Some(variants);
            }
        }
        None
    });

    assert!(tagged_def.is_some(), "Should have DevActionType definition");
    let variants = tagged_def.unwrap();
    assert_eq!(variants.len(), 1, "Should have one variant");

    let navigate_variant = &variants[0];
    assert_eq!(
        navigate_variant.name, "Navigate",
        "Variant should be named Navigate"
    );

    // Check the fields
    let fields = navigate_variant.fields.as_ref().unwrap();
    let columns: Vec<_> = fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(columns.len(), 1, "Should have one field");
    assert_eq!(columns[0].name, "to", "Field should be named 'to'");
    assert_eq!(columns[0].type_, "String", "Field should be of type String");
}

#[test]
fn test_comma_delimited_fields() {
    let schema_source = r#"
type DevActionType
    = Type { selector String, text String, clear Bool? }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Comma-delimited fields should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the type was parsed correctly
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];

    // Find the DevActionType definition
    let tagged_def = file.definitions.iter().find_map(|d| {
        if let ast::Definition::Tagged { name, variants, .. } = d {
            if name == "DevActionType" {
                return Some(variants);
            }
        }
        None
    });

    assert!(tagged_def.is_some(), "Should have DevActionType definition");
    let variants = tagged_def.unwrap();
    assert_eq!(variants.len(), 1, "Should have one variant");

    let type_variant = &variants[0];
    assert_eq!(type_variant.name, "Type", "Variant should be named Type");

    // Check the fields
    let fields = type_variant.fields.as_ref().unwrap();
    let columns: Vec<_> = fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(columns.len(), 3, "Should have three fields");

    assert_eq!(
        columns[0].name, "selector",
        "First field should be 'selector'"
    );
    assert_eq!(columns[0].type_, "String", "First field should be String");
    assert_eq!(
        columns[0].nullable, false,
        "First field should not be nullable"
    );

    assert_eq!(columns[1].name, "text", "Second field should be 'text'");
    assert_eq!(columns[1].type_, "String", "Second field should be String");
    assert_eq!(
        columns[1].nullable, false,
        "Second field should not be nullable"
    );

    assert_eq!(columns[2].name, "clear", "Third field should be 'clear'");
    assert_eq!(columns[2].type_, "Bool", "Third field should be Bool");
    assert_eq!(columns[2].nullable, true, "Third field should be nullable");
}

#[test]
fn test_mixed_variant_formats() {
    let schema_source = r#"
type ActionType
    = Navigate { to String }
    | Type { selector String, text String }
    | Click {
        elementId Int
        modifier String?
      }
    | Simple
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Mixed variant formats should parse successfully. Error: {:?}",
        result.err()
    );

    // Verify the type was parsed correctly
    assert_eq!(schema.files.len(), 1, "Should have one schema file");
    let file = &schema.files[0];

    // Find the ActionType definition
    let tagged_def = file.definitions.iter().find_map(|d| {
        if let ast::Definition::Tagged { name, variants, .. } = d {
            if name == "ActionType" {
                return Some(variants);
            }
        }
        None
    });

    assert!(tagged_def.is_some(), "Should have ActionType definition");
    let variants = tagged_def.unwrap();
    assert_eq!(variants.len(), 4, "Should have four variants");

    // Check Navigate variant (single field on same line)
    assert_eq!(variants[0].name, "Navigate");
    let navigate_fields = variants[0].fields.as_ref().unwrap();
    let navigate_columns: Vec<_> = navigate_fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(navigate_columns.len(), 1);
    assert_eq!(navigate_columns[0].name, "to");

    // Check Type variant (comma-delimited fields)
    assert_eq!(variants[1].name, "Type");
    let type_fields = variants[1].fields.as_ref().unwrap();
    let type_columns: Vec<_> = type_fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(type_columns.len(), 2);
    assert_eq!(type_columns[0].name, "selector");
    assert_eq!(type_columns[1].name, "text");

    // Check Click variant (multi-line fields)
    assert_eq!(variants[2].name, "Click");
    let click_fields = variants[2].fields.as_ref().unwrap();
    let click_columns: Vec<_> = click_fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(click_columns.len(), 2);
    assert_eq!(click_columns[0].name, "elementId");
    assert_eq!(click_columns[1].name, "modifier");

    // Check Simple variant (no fields)
    assert_eq!(variants[3].name, "Simple");
    assert!(variants[3].fields.is_none());
}

#[test]
fn test_single_field_with_nullable() {
    let schema_source = r#"
type Result
    = Success { message String? }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Single nullable field should parse successfully. Error: {:?}",
        result.err()
    );

    let file = &schema.files[0];
    let variants = file
        .definitions
        .iter()
        .find_map(|d| {
            if let ast::Definition::Tagged { name, variants, .. } = d {
                if name == "Result" {
                    Some(variants)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    let fields = variants[0].fields.as_ref().unwrap();
    let columns: Vec<_> = fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(columns[0].nullable, true, "Field should be nullable");
}

#[test]
fn test_comma_separated_with_optional_trailing_comma() {
    let schema_source = r#"
type Point
    = TwoD { x Int, y Int }
    | ThreeD { x Int, y Int, z Int }
    "#;

    let mut schema = ast::Schema::default();
    let result = parser::run("schema.pyre", schema_source, &mut schema);
    assert!(
        result.is_ok(),
        "Comma-separated fields should parse successfully. Error: {:?}",
        result.err()
    );

    let file = &schema.files[0];
    let variants = file
        .definitions
        .iter()
        .find_map(|d| {
            if let ast::Definition::Tagged { name, variants, .. } = d {
                if name == "Point" {
                    Some(variants)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    // Check TwoD variant
    let twod_fields = variants[0].fields.as_ref().unwrap();
    let twod_columns: Vec<_> = twod_fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(twod_columns.len(), 2);

    // Check ThreeD variant
    let threed_fields = variants[1].fields.as_ref().unwrap();
    let threed_columns: Vec<_> = threed_fields
        .iter()
        .filter_map(|f| {
            if let ast::Field::Column(col) = f {
                Some(col)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(threed_columns.len(), 3);
}
