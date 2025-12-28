use pyre::ast;
use pyre::parser;
use pyre::typecheck;

fn check_schema_and_get_layers(schema_source: &str) -> std::collections::HashMap<String, usize> {
    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema)
        .expect("Failed to parse schema");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let context = typecheck::check_schema(&database).expect("Failed to typecheck schema");

    // Extract sync layers for each table
    let mut layers = std::collections::HashMap::new();
    for (record_name, table) in &context.tables {
        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
        layers.insert(table_name, table.sync_layer);
    }

    layers
}

#[test]
fn test_simple_linear_dependency() {
    // A -> B -> C
    // Expected: A=0, B=1, C=2
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    bId Int
    b @link(bId, B.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&2), "C should be layer 2");
}

#[test]
fn test_multiple_dependencies() {
    // A -> B, A -> C
    // Expected: A=0, B=1, C=1
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&1), "C should be layer 1");
}

#[test]
fn test_circular_dependency() {
    // A <-> B (circular)
    // Expected: A=0, B=0 (same layer due to cycle)
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    bId Int?
    b @link(bId, B.id)
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int?
    a @link(aId, A.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    let a_layer = layers.get("a").expect("Table 'a' should exist");
    let b_layer = layers.get("b").expect("Table 'b' should exist");
    
    assert_eq!(a_layer, b_layer, "A and B should have the same layer due to circular dependency");
    assert_eq!(a_layer, &0, "Circular dependency should be in layer 0");
}

#[test]
fn test_independent_tables() {
    // A, B, C (no dependencies)
    // Expected: A=0, B=0, C=0
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    @public
}

record C {
    @tablename "c"
    id Int @id
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&0), "B should be layer 0");
    assert_eq!(layers.get("c"), Some(&0), "C should be layer 0");
}

#[test]
fn test_complex_graph() {
    // A -> B -> D
    // A -> C -> D
    // Expected: A=0, B=1, C=1, D=2
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record D {
    @tablename "d"
    id Int @id
    bId Int?
    b @link(bId, B.id)
    cId Int?
    c @link(cId, C.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&1), "C should be layer 1");
    assert_eq!(layers.get("d"), Some(&2), "D should be layer 2");
}

#[test]
fn test_three_way_cycle() {
    // A -> B -> C -> A (cycle)
    // Expected: A=0, B=0, C=0 (all same layer)
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    bId Int?
    b @link(bId, B.id)
    @public
}

record B {
    @tablename "b"
    id Int @id
    cId Int?
    c @link(cId, C.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    aId Int?
    a @link(aId, A.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    let a_layer = layers.get("a").expect("Table 'a' should exist");
    let b_layer = layers.get("b").expect("Table 'b' should exist");
    let c_layer = layers.get("c").expect("Table 'c' should exist");

    assert_eq!(a_layer, b_layer, "A and B should have the same layer");
    assert_eq!(b_layer, c_layer, "B and C should have the same layer");
    assert_eq!(a_layer, &0, "Cycle should be in layer 0");
}

#[test]
fn test_cycle_with_external_dependency() {
    // A -> B <-> C (B and C cycle, A depends on nothing)
    // D -> B (D depends on B)
    // Expected: A=0, B=0, C=0 (cycle), D=1
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    cId Int?
    c @link(cId, C.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    bId Int?
    b @link(bId, B.id)
    @public
}

record D {
    @tablename "d"
    id Int @id
    bId Int
    b @link(bId, B.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    
    let b_layer = layers.get("b").expect("Table 'b' should exist");
    let c_layer = layers.get("c").expect("Table 'c' should exist");
    assert_eq!(b_layer, c_layer, "B and C should have the same layer (cycle)");
    assert_eq!(b_layer, &0, "Cycle should be in layer 0");
    
    assert_eq!(layers.get("d"), Some(&1), "D should be layer 1 (depends on B in cycle)");
}

#[test]
fn test_deep_nested_dependencies() {
    // A -> B -> C -> D -> E
    // Expected: A=0, B=1, C=2, D=3, E=4
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    bId Int
    b @link(bId, B.id)
    @public
}

record D {
    @tablename "d"
    id Int @id
    cId Int
    c @link(cId, C.id)
    @public
}

record E {
    @tablename "e"
    id Int @id
    dId Int
    d @link(dId, D.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&2), "C should be layer 2");
    assert_eq!(layers.get("d"), Some(&3), "D should be layer 3");
    assert_eq!(layers.get("e"), Some(&4), "E should be layer 4");
}

#[test]
fn test_multiple_links_same_table() {
    // A -> B (via link1)
    // A -> B (via link2)
    // Expected: A=0, B=1
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId1 Int
    a1 @link(aId1, A.id)
    aId2 Int
    a2 @link(aId2, A.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
}

#[test]
fn test_table_with_no_links() {
    // A has links, B has no links
    // Expected: Both should have valid layers
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    name String
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&0), "C should be layer 0 (no dependencies)");
}

#[test]
fn test_diamond_pattern() {
    //   A
    //  / \
    // B   C
    //  \ /
    //   D
    // Expected: A=0, B=1, C=1, D=2
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    @public
}

record B {
    @tablename "b"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record C {
    @tablename "c"
    id Int @id
    aId Int
    a @link(aId, A.id)
    @public
}

record D {
    @tablename "d"
    id Int @id
    bId Int?
    b @link(bId, B.id)
    cId Int?
    c @link(cId, C.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0");
    assert_eq!(layers.get("b"), Some(&1), "B should be layer 1");
    assert_eq!(layers.get("c"), Some(&1), "C should be layer 1");
    assert_eq!(layers.get("d"), Some(&2), "D should be layer 2");
}

#[test]
fn test_self_referential_table() {
    // A -> A (self-reference)
    // Expected: A=0 (self-cycle)
    let schema = r#"
record A {
    @tablename "a"
    id Int @id
    parentId Int?
    parent @link(parentId, A.id)
    @public
}
"#;

    let layers = check_schema_and_get_layers(schema);

    assert_eq!(layers.get("a"), Some(&0), "A should be layer 0 (self-cycle)");
}

