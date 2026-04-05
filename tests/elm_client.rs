use pyre::ast;
use pyre::filesystem::GeneratedFile;
use pyre::generate::client::elm;
use pyre::parser;
use pyre::typecheck;
use std::path::Path;

#[test]
fn generated_pyre_elm_uses_query_upserts() {
    let schema_source = r#"
record Rulebook {
    @public

    id   Id.Int @id
    name String
}

record GameWorld {
    @public

    id   Id.Int @id
    slug String
}
"#;

    let query_source = r#"
query GetRulebookByName($name: String) {
    rulebook {
        @where { name == $name }

        id
        name
    }
}

query GetGameWorld($slug: String) {
    gameWorld {
        @where { slug == $slug }

        id
        slug
    }
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };
    let context = typecheck::check_schema(&database).expect("schema typechecks");

    let query_list = parser::parse_query("query.pyre", query_source).expect("query parses");
    typecheck::check_queries(&query_list, &context).expect("query typechecks");

    let mut files: Vec<GeneratedFile<String>> = Vec::new();
    elm::generate_queries(&context, &query_list, Path::new("client/elm"), &mut files);

    let generated = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Pyre.elm"))
        .expect("generated Pyre.elm file");

    let content = &generated.contents;

    assert!(
        content.contains("module Pyre exposing (Model, QueryModel, Query(..), Msg(..), Effect(..), init, update, decodeIncomingDelta, getResult)"),
        "Pyre.elm should expose QueryModel and Query constructors. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type Query\n    = GetRulebookByName String Query.GetRulebookByName.Input\n    | GetGameWorld String Query.GetGameWorld.Input"),
        "Pyre.elm should generate a Query type for outbound upserts. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type Msg\n    = QueryUpdate Query\n    | GetRulebookByName_DataReceived String Query.GetRulebookByName.QueryDelta\n    | GetRulebookByName_Unregistered String\n    | GetGameWorld_DataReceived String Query.GetGameWorld.QueryDelta\n    | GetGameWorld_Unregistered String"),
        "Pyre.elm should collapse register/update into QueryUpdate. Generated:\n{}",
        content
    );
    assert!(
        !content.contains("_Registered") && !content.contains("_InputUpdated"),
        "Pyre.elm should not generate separate register/update constructors anymore. Generated:\n{}",
        content
    );
    assert!(
        content.contains("update msg model =\n    case msg of\n        QueryUpdate query ->\n            updateQuery query model")
            && content.contains("updateQuery : Query -> Model -> ( Model, Effect )\nupdateQuery query model =\n    case query of\n        GetRulebookByName queryId input ->")
            && content.contains("        GetGameWorld queryId input ->"),
        "Pyre.elm should delegate QueryUpdate handling to updateQuery. Generated:\n{}",
        content
    );
    assert!(
        content.contains("incomingDeltaDecoder =\n    Decode.map2 Tuple.pair\n        (Decode.field \"queryName\" Decode.string)\n        (Decode.field \"queryId\" Decode.string)")
            && !content.contains("Decode.field \"querySource\" Decode.string"),
        "Pyre.elm should decode queryName, not querySource, for inbound result routing. Generated:\n{}",
        content
    );
    assert!(
        content.contains("Just queryModel ->\n                    ( { model | getRulebookByName = Dict.insert queryId { queryModel | input = input } model.getRulebookByName }\n                    , Send (encodeUpdateInput queryId Query.GetRulebookByName.queryShape (Query.GetRulebookByName.encode input))")
            && content.contains("Nothing ->\n                    let\n                        queryModel =\n                            { input = input, result = Query.GetRulebookByName.ReturnData [], revision = 0 }\n                    in\n                    ( { model | getRulebookByName = Dict.insert queryId queryModel model.getRulebookByName }\n                    , Send (encodeRegister \"GetRulebookByName\" Query.GetRulebookByName.queryShape queryId (Query.GetRulebookByName.encode input))")
            && content.contains("encodeRegister : String -> Encode.Value -> String -> Encode.Value -> Encode.Value\nencodeRegister queryName queryShape queryId input =\n    Encode.object\n        [ ( \"type\", Encode.string \"register\" )\n        , ( \"queryName\", Encode.string queryName )\n        , ( \"querySource\", queryShape )")
            && content.contains("encodeUpdateInput : String -> Encode.Value -> Encode.Value -> Encode.Value\nencodeUpdateInput queryId queryShape input =\n    Encode.object\n        [ ( \"type\", Encode.string \"update-input\" )\n        , ( \"queryId\", Encode.string queryId )\n        , ( \"querySource\", queryShape )"),
        "Pyre.elm should upsert queries by id. Generated:\n{}",
        content
    );
}
