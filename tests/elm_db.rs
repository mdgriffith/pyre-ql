use pyre::ast;
use pyre::generate::client::elm;
use pyre::parser;

#[test]
fn db_elm_tagged_record_variant_ignores_comments_when_formatting_first_field() {
    let schema_source = r#"
type TileFormat
   = Png
   | Webp

type Tiling = Tiling {
    // Storage prefix for tiles; full tile key is {tileRootKey}/{z}/{x}/{y}.{format}
    tileRootKey       String
    tileWidth         Int
    tileHeight        Int
    highestTileLevel  Int
    format            TileFormat
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let generated = elm::write_schema(&database);

    assert!(
        generated.contains("type Tiling\n    = Tiling\n      { tileRootKey : String"),
        "Expected first field in variant record without leading comma. Generated:\n{}",
        generated
    );
    assert!(
        !generated.contains("{       , tileRootKey"),
        "Generated Db.elm should not contain malformed leading comma. Generated:\n{}",
        generated
    );
}

#[test]
fn db_elm_json_alias_and_json_codecs_are_generated() {
    let schema_source = r#"
record GameAsset {
    @public
    id    Id.Int @id
    attrs Json
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let db_module = elm::write_schema(&database);
    let decode_module = elm::to_schema_decoders(&database);
    let encode_module = elm::to_schema_encoders(&database);

    assert!(db_module.contains("import Json.Encode"));
    assert!(db_module.contains("type alias Json =\n    Json.Encode.Value"));

    assert!(decode_module.contains("json : Decode.Decoder Json"));
    assert!(decode_module.contains("json =\n    Decode.value"));

    assert!(encode_module.contains("json : Db.Json -> Encode.Value"));
    assert!(encode_module.contains("json value =\n    value"));
}
