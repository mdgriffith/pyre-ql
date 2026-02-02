module Main exposing (main)

import Browser
import Html exposing (Html, div, h1, li, p, text, ul)
import Html.Attributes exposing (style)
import Pyre
import Query.ListUsersAndPosts



-- MODEL


type alias Model =
    { pyre : Pyre.Model
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { pyre = Pyre.init }
    , Cmd.none
    )



-- MSG


type Msg
    = PyreMsg Pyre.Msg
    | RegisterQuery



-- UPDATE


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        PyreMsg pyreMsg ->
            let
                ( newPyre, pyreCmd ) =
                    Pyre.update pyreMsg model.pyre
            in
            ( { model | pyre = newPyre }
            , Cmd.map PyreMsg pyreCmd
            )

        RegisterQuery ->
            -- Example: register a query with id "main-query"
            let
                ( newPyre, pyreCmd ) =
                    Pyre.update
                        (Pyre.ListUsersAndPosts_Registered "main-query" {})
                        model.pyre
            in
            ( { model | pyre = newPyre }
            , Cmd.map PyreMsg pyreCmd
            )



-- VIEW


view : Model -> Html Msg
view model =
    div [ style "padding" "20px", style "font-family" "sans-serif" ]
        [ h1 [] [ text "Pyre QueryClient Demo" ]
        , viewQueryResult model
        ]


viewQueryResult : Model -> Html Msg
viewQueryResult model =
    case Pyre.getResult "main-query" model.pyre.listUsersAndPosts of
        Just result ->
            div []
                [ h2 [] [ text "Users" ]
                , ul [] (List.map viewUser result.user)
                , h2 [] [ text "Posts" ]
                , ul [] (List.map viewPost result.post)
                ]

        Nothing ->
            p [] [ text "No query registered yet" ]


h2 : List (Html.Attribute msg) -> List (Html msg) -> Html msg
h2 =
    Html.h2


viewUser : Query.ListUsersAndPosts.User -> Html Msg
viewUser user =
    li []
        [ text ("User #" ++ String.fromInt user.id ++ ": " ++ Maybe.withDefault "(no name)" user.name)
        ]


viewPost : Query.ListUsersAndPosts.Post -> Html Msg
viewPost post =
    li []
        [ text ("Post #" ++ String.fromInt post.id ++ ": " ++ post.title)
        ]



-- SUBSCRIPTIONS


subscriptions : Model -> Sub Msg
subscriptions _ =
    Sub.map PyreMsg Pyre.subscriptions



-- MAIN


main : Program () Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , view = view
        , subscriptions = subscriptions
        }

