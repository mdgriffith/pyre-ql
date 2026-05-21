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
    let query_info = typecheck::check_queries(&query_list, &context).expect("query typechecks");

    let mut files: Vec<GeneratedFile<String>> = Vec::new();
    elm::generate_queries(
        &context,
        &query_info,
        &query_list,
        Path::new("client/elm"),
        &mut files,
    );

    let generated = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Pyre.elm"))
        .expect("generated Pyre.elm file");

    let content = &generated.contents;

    assert!(
        content.contains("module Pyre exposing (DatabaseId, Default, QueryId, Model, QueryModel, Query(..), Msg(..), Effect(..), init, update, decodeIncomingDelta, getResult)"),
        "Pyre.elm should expose QueryModel and Query constructors. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type alias DatabaseId namespace =\n    Db.Database.DatabaseId namespace\n\n\ntype alias Default =\n    Db.Database.Default\n\n\ntype alias QueryId =\n    String")
            && content.contains("type Query\n    = GetRulebookByName (DatabaseId Default) QueryId Query.GetRulebookByName.Input\n    | GetGameWorld (DatabaseId Default) QueryId Query.GetGameWorld.Input"),
        "Pyre.elm should generate a Query type for outbound upserts. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type Msg\n    = QueryUpdate Query\n    | GetRulebookByName_DataReceived QueryId Query.GetRulebookByName.QueryDelta\n    | GetRulebookByName_Unregistered (DatabaseId Default) QueryId\n    | GetGameWorld_DataReceived QueryId Query.GetGameWorld.QueryDelta\n    | GetGameWorld_Unregistered (DatabaseId Default) QueryId"),
        "Pyre.elm should collapse register/update into QueryUpdate. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type Effect\n    = NoEffect\n    | Send Encode.Value\n    | QueryUpdated QueryId\n    | LogError Encode.Value"),
        "Pyre.elm should expose query update effects. Generated:\n{}",
        content
    );
    assert!(
        content.contains(", QueryUpdated queryId\n                            )"),
        "Pyre.elm should return QueryUpdated after applying query deltas. Generated:\n{}",
        content
    );
    assert!(
        !content.contains("_Registered") && !content.contains("_InputUpdated"),
        "Pyre.elm should not generate separate register/update constructors anymore. Generated:\n{}",
        content
    );
    assert!(
        content.contains("update msg model =\n    case msg of\n        QueryUpdate query ->\n            updateQuery query model")
            && content.contains("updateQuery : Query -> Model -> ( Model, Effect )\nupdateQuery query model =\n    case query of\n        GetRulebookByName databaseId queryId input ->")
            && content.contains("        GetGameWorld databaseId queryId input ->"),
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
        content.contains("Just queryModel ->\n                    ( { model | getRulebookByName = Dict.insert queryId { queryModel | input = input } model.getRulebookByName }\n                    , Send (encodeUpdateInput databaseId queryId Query.GetRulebookByName.queryShape (Query.GetRulebookByName.encode input))")
            && content.contains("Nothing ->\n                    let\n                        queryModel =\n                            { input = input, result = Query.GetRulebookByName.ReturnData [], revision = 0 }\n                    in\n                    ( { model | getRulebookByName = Dict.insert queryId queryModel model.getRulebookByName }\n                    , Send (encodeRegister databaseId \"GetRulebookByName\" Query.GetRulebookByName.queryShape queryId (Query.GetRulebookByName.encode input))")
            && content.contains("encodeRegister : DatabaseId namespace -> String -> Encode.Value -> QueryId -> Encode.Value -> Encode.Value\nencodeRegister databaseId queryName queryShape queryId input =\n    Encode.object\n        [ ( \"type\", Encode.string \"register\" )\n        , ( \"databaseId\", Db.Database.encode databaseId )\n        , ( \"queryName\", Encode.string queryName )\n        , ( \"querySource\", queryShape )")
            && content.contains("encodeUpdateInput : DatabaseId namespace -> QueryId -> Encode.Value -> Encode.Value -> Encode.Value\nencodeUpdateInput databaseId queryId queryShape input =\n    Encode.object\n        [ ( \"type\", Encode.string \"update-input\" )\n        , ( \"databaseId\", Db.Database.encode databaseId )\n        , ( \"queryId\", Encode.string queryId )\n        , ( \"querySource\", queryShape )"),
        "Pyre.elm should upsert queries by id. Generated:\n{}",
        content
    );
}

#[test]
fn generated_elm_mutation_modules_include_bridge_metadata() {
    let schema_source = r#"
record Post {
    @public

    id    Id.Int @id
    title String
}
"#;

    let query_source = r#"
insert CreatePost($title: String) {
    post {
        title = $title
        id
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
    let query_info = typecheck::check_queries(&query_list, &context).expect("query typechecks");

    let mut files: Vec<GeneratedFile<String>> = Vec::new();
    elm::generate_queries(
        &context,
        &query_info,
        &query_list,
        Path::new("client/elm"),
        &mut files,
    );

    let generated = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Query/CreatePost.elm"))
        .expect("generated CreatePost.elm file");

    let content = &generated.contents;

    assert!(
        content.contains("module Query.CreatePost exposing (encode, DatabaseId, Default, RequestId, id, name, mutationRequest, decodeMutationResult, MutationResult, Input, Post, ReturnData)"),
        "Mutation modules should expose bridge metadata helpers. Generated:\n{}",
        content
    );
    assert!(
        content.contains("id : String\nid =\n    \"")
            && content.contains("name : String\nname =\n    \"CreatePost\"")
            && content.contains("type alias DatabaseId namespace =\n    Db.Database.DatabaseId namespace\n\n\ntype alias RequestId =\n    String")
            && content.contains("type alias Default =\n    Db.Database.Default")
            && content.contains("mutationRequest : (DatabaseId Default) -> RequestId -> Input -> Encode.Value\nmutationRequest databaseId requestId input =\n    Encode.object\n        [ ( \"type\", Encode.string \"mutate\" )\n        , ( \"databaseId\", Db.Database.encode databaseId )\n        , ( \"requestId\", Encode.string requestId )\n        , ( \"mutationId\", Encode.string id )\n        , ( \"mutationName\", Encode.string name )\n        , ( \"mutationInput\", encode input )\n        ]")
            && content.contains("type alias MutationResult =\n    { requestId : RequestId\n    , mutationId : String\n    , mutationName : Maybe String\n    , result : Result String ReturnData\n    }")
            && content.contains("decodeMutationResult : Decode.Decoder MutationResult")
            && content.contains("decodeBridgeMutationResult : Decode.Decoder value -> Decode.Decoder (Result String value)"),
        "Mutation modules should emit request payloads and typed result decoders. Generated:\n{}",
        content
    );
}

#[test]
fn generated_entity_stream_module_encodes_id_filtered_streams() {
    let schema_source = r#"
record Post {
    @public

    id    Id.Int @id
    title String
}

record Comment {
    @public

    id     Id.Uuid @id
    postId Post.id
    body   String
}
"#;

    let mut schema = ast::Schema::default();
    parser::run("schema.pyre", schema_source, &mut schema).expect("schema parses");

    let database = ast::Database {
        schemas: vec![schema],
    };

    let mut files: Vec<GeneratedFile<String>> = Vec::new();
    elm::generate(Path::new("client/elm"), &database, &mut files);

    let generated = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("EntityStream.elm"))
        .expect("generated EntityStream.elm file");

    let content = &generated.contents;

    assert!(
        content.contains("module EntityStream exposing (DatabaseId, EntityChange(..), EntityChangeBatch, EntityChangeBatchSource(..), EntitySubscription(..), StreamId, decodeIncomingBatch, register, unregister)"),
        "EntityStream.elm should expose the slim bridge API. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type EntitySubscription\n    = Comment (Maybe (List String))\n    | Post (Maybe (List Int))"),
        "EntityStream.elm should generate table constructors with optional ID filters. Generated:\n{}",
        content
    );
    assert!(
        content.contains("register : DatabaseId namespace -> StreamId -> List EntitySubscription -> Encode.Value")
            && content.contains("( \"type\", Encode.string \"register-entity-stream\" )")
            && content.contains("( \"tables\", Encode.list encodeSubscription subscriptions )"),
        "EntityStream.elm should encode register-entity-stream payloads. Generated:\n{}",
        content
    );
    assert!(
        content.contains(
            "Comment ids ->\n            tableSubscription \"comments\" ids Encode.string"
        ) && content
            .contains("Post ids ->\n            tableSubscription \"posts\" ids Encode.int")
            && content.contains("[ ( \"$in\", Encode.list encodeId values ) ]"),
        "EntityStream.elm should encode ID $in filters for each table. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type alias CommentEntity =\n    { id : String\n    , postId : Int\n    , body : String\n    }")
            && content.contains("type alias PostEntity =\n    { id : Int\n    , title : String\n    }"),
        "EntityStream.elm should generate typed row aliases. Generated:\n{}",
        content
    );
    assert!(
        content.contains("type EntityChange\n    = CommentRow CommentEntity\n    | PostRow PostEntity\n    | EntityDecodeFailed String Decode.Value")
            && content.contains("decodeComment : Decode.Decoder CommentEntity")
            && content.contains("|> Db.Decode.andField \"id\" Decode.string")
            && content.contains("|> Db.Decode.andField \"postId\" Decode.int")
            && content.contains("decodePost : Decode.Decoder PostEntity")
            && content.contains("\"comments\" ->\n                        decodeRow CommentRow decodeComment row")
            && content.contains("\"posts\" ->\n                        decodeRow PostRow decodePost row"),
        "EntityStream.elm should decode batches into typed table row variants. Generated:\n{}",
        content
    );
}
