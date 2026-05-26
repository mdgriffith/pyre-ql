use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::shared::Options;
use crate::db;
use pyre::ast;
use pyre::server::manifest::{FieldSchema, Manifest, PyreSession, QueryManifest, SqlInfo};

const SERVER_NAME: &str = "pyre";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

pub async fn mcp(options: &Options<'_>) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_message(&mut reader)? {
        let request = match serde_json::from_slice::<JsonValue>(&message) {
            Ok(request) => request,
            Err(error) => {
                write_message(&mut writer, &parse_error(error.to_string()))?;
                continue;
            }
        };

        if let Some(response) = handle_message(options, request).await {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

async fn handle_message(options: &Options<'_>, request: JsonValue) -> Option<JsonValue> {
    let id = request.get("id")?.clone();
    let method = request.get("method").and_then(JsonValue::as_str);

    Some(match method {
        Some("initialize") => success_response(id, initialize_result(&request)),
        Some("ping") => success_response(id, json!({})),
        Some("tools/list") => success_response(id, json!({ "tools": tools() })),
        Some("resources/list") => success_response(id, json!({ "resources": [] })),
        Some("prompts/list") => success_response(id, json!({ "prompts": [] })),
        Some("tools/call") => match call_tool(options, request.get("params")).await {
            Ok(result) => success_response(id, result),
            Err(message) => error_response(id, -32603, &message),
        },
        Some(method) => error_response(id, -32601, &format!("Unknown method: {method}")),
        None => error_response(id, -32600, "Request is missing method"),
    })
}

fn initialize_result(request: &JsonValue) -> JsonValue {
    let protocol_version = request
        .get("params")
        .and_then(|params| params.get("protocolVersion"))
        .and_then(JsonValue::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);

    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    })
}

async fn call_tool(options: &Options<'_>, params: Option<&JsonValue>) -> Result<JsonValue, String> {
    let params = params.ok_or_else(|| "tools/call params are required".to_string())?;
    let name = params
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| "tools/call params.name is required".to_string())?;
    let arguments = params.get("arguments").unwrap_or(&JsonValue::Null);

    let value = match name {
        "pyre_project_info" => project_info(options, arguments)?,
        "pyre_docs" => docs(arguments)?,
        "pyre_schema" => schema(arguments)?,
        "pyre_check" => run_cli(options, arguments, &["check", "--json"]),
        "pyre_format" => run_format(options, arguments)?,
        "pyre_generate" => run_generate(options, arguments)?,
        "pyre_generate_migration" => run_generate_migration(options, arguments)?,
        "pyre_migrate" => run_migrate(options, arguments)?,
        "pyre_db_status" => db_status(options, arguments).await?,
        "pyre_query" => dynamic_query(options, arguments).await?,
        _ => return Err(format!("Unknown Pyre MCP tool: {name}")),
    };

    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
        }]
    }))
}

fn project_info(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let in_dir = input_dir(options, arguments);
    let found = crate::filesystem::collect_filepaths(&in_dir).map_err(|error| error.to_string())?;
    let generated_dir =
        string_arg(arguments, "generated").unwrap_or_else(|| "pyre/generated".to_string());
    let migration_dir =
        string_arg(arguments, "migration_dir").unwrap_or_else(|| "pyre/migrations".to_string());

    Ok(json!({
        "inputDir": in_dir,
        "generatedDir": generated_dir,
        "generatedDirExists": Path::new(&generated_dir).exists(),
        "migrationDir": migration_dir,
        "migrationDirExists": Path::new(&migration_dir).exists(),
        "namespaces": sorted_keys(&found.schema_files),
        "schemaFileCount": found.schema_files.values().map(Vec::len).sum::<usize>(),
        "queryFileCount": found.query_files.len(),
        "queryFiles": found.query_files
    }))
}

fn docs(arguments: &JsonValue) -> Result<JsonValue, String> {
    let topic = string_arg(arguments, "topic").unwrap_or_else(|| "overview".to_string());
    let content = match topic.as_str() {
        "overview" => include_str!("../../../README.md"),
        "docs" => include_str!("../../../docs/README.md"),
        "usage" => include_str!("../../../docs/usage/README.md"),
        "dev" => include_str!("../../../docs/dev/README.md"),
        "cli" => include_str!("../../../packages/cli/README.md"),
        _ => return Err(format!("Unknown docs topic: {topic}")),
    };

    Ok(json!({ "topic": topic, "content": content }))
}

fn schema(arguments: &JsonValue) -> Result<JsonValue, String> {
    let in_dir = string_arg(arguments, "in").unwrap_or_else(|| "pyre".to_string());
    let found = crate::filesystem::collect_filepaths(Path::new(&in_dir))
        .map_err(|error| error.to_string())?;
    let mut schemas = Vec::new();
    let mut namespaces = found.schema_files.into_iter().collect::<Vec<_>>();
    namespaces.sort_by(|left, right| left.0.cmp(&right.0));

    for (namespace, files) in namespaces {
        for file in files {
            schemas.push(json!({
                "namespace": namespace,
                "path": file.path,
                "content": file.content
            }));
        }
    }

    Ok(json!({ "schemas": schemas }))
}

fn run_format(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let mut args = vec!["format".to_string()];
    if bool_arg(arguments, "to_stdout").unwrap_or(false) {
        args.push("--to-stdout".to_string());
    }
    args.extend(string_array_arg(arguments, "files")?);
    Ok(run_cli(
        options,
        arguments,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

fn run_generate(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let out = string_arg(arguments, "out").unwrap_or_else(|| "pyre/generated".to_string());
    Ok(run_cli(options, arguments, &["generate", "--out", &out]))
}

fn run_generate_migration(
    options: &Options<'_>,
    arguments: &JsonValue,
) -> Result<JsonValue, String> {
    let name = required_string_arg(arguments, "name")?;
    let db = required_string_arg(arguments, "database")?;
    let migration_dir =
        string_arg(arguments, "migration_dir").unwrap_or_else(|| "pyre/migrations".to_string());
    let mut args = vec![
        "migration".to_string(),
        name,
        "--db".to_string(),
        db,
        "--migration-dir".to_string(),
        migration_dir,
    ];
    push_optional_arg(arguments, &mut args, "auth", "--auth");
    push_optional_arg(arguments, &mut args, "namespace", "--namespace");
    Ok(run_cli(
        options,
        arguments,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

fn run_migrate(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let database = required_string_arg(arguments, "database")?;
    let migration_dir =
        string_arg(arguments, "migration_dir").unwrap_or_else(|| "pyre/migrations".to_string());
    let mut args = vec![
        "migrate".to_string(),
        database,
        "--migration-dir".to_string(),
        migration_dir,
    ];
    push_optional_arg(arguments, &mut args, "auth", "--auth");
    push_optional_arg(arguments, &mut args, "namespace", "--namespace");
    if bool_arg(arguments, "push").unwrap_or(false) {
        args.push("--push".to_string());
    }
    Ok(run_cli(
        options,
        arguments,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

fn run_cli(options: &Options<'_>, arguments: &JsonValue, command_args: &[&str]) -> JsonValue {
    let in_dir = input_dir(options, arguments);
    let output = Command::new(std::env::current_exe().unwrap_or_else(|_| PathBuf::from("pyre")))
        .arg("--in")
        .arg(in_dir)
        .args(command_args)
        .output();

    match output {
        Ok(output) => json!({
            "ok": output.status.success(),
            "status": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr)
        }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    }
}

async fn db_status(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let database = required_string_arg(arguments, "database")?;
    let auth = string_arg(arguments, "auth");
    let namespace =
        string_arg(arguments, "namespace").unwrap_or_else(|| ast::DEFAULT_SCHEMANAME.to_string());
    let migration_dir =
        string_arg(arguments, "migration_dir").unwrap_or_else(|| "pyre/migrations".to_string());
    let namespace_migration_dir = if namespace == ast::DEFAULT_SCHEMANAME {
        PathBuf::from(&migration_dir)
    } else {
        Path::new(&migration_dir).join(&namespace)
    };

    let db = match db::connect(&database, &auth).await {
        Ok(db) => db,
        Err(error) => {
            return Ok(json!({
                "ok": false,
                "accessible": false,
                "status": "inaccessible",
                "error": error.format_error()
            }))
        }
    };
    let conn = db.connect().map_err(|error| error.to_string())?;
    let introspection = crate::db::introspect::introspect_connection(&conn)
        .await
        .map_err(|error| error.to_string())?;

    let migration_files = db::read_migration_items(&namespace_migration_dir).unwrap_or_default();
    let applied_migrations = match &introspection.migration_state {
        pyre::db::introspect::MigrationState::NoMigrationTable => Vec::new(),
        pyre::db::introspect::MigrationState::MigrationTable { migrations } => migrations
            .iter()
            .map(|migration| migration.name.clone())
            .collect::<Vec<_>>(),
    };
    let pending_migrations = migration_files
        .iter()
        .filter(|name| !applied_migrations.contains(name))
        .cloned()
        .collect::<Vec<_>>();

    let mut schema_status = json!({ "checked": false });
    let status = if !pending_migrations.is_empty() {
        "pending_migrations"
    } else {
        match current_schema_context(options, arguments) {
            Ok((database_schema, context, _paths)) => {
                if let Some(schema) = database_schema
                    .schemas
                    .iter()
                    .find(|schema| schema.namespace == namespace)
                {
                    let diff = pyre::db::diff::diff(&context, schema, &introspection);
                    let up_to_date = pyre::db::diff::is_empty(&diff);
                    schema_status =
                        json!({ "checked": true, "upToDate": up_to_date, "diff": diff });
                    if up_to_date {
                        "up_to_date"
                    } else {
                        "schema_drift"
                    }
                } else {
                    schema_status = json!({ "checked": false, "error": format!("namespace not found: {namespace}") });
                    "unknown"
                }
            }
            Err(error) => {
                schema_status = json!({ "checked": false, "error": error });
                "unknown"
            }
        }
    };

    Ok(json!({
        "ok": true,
        "accessible": true,
        "status": status,
        "namespace": namespace,
        "appliedMigrations": applied_migrations,
        "pendingMigrations": pending_migrations,
        "schema": schema_status
    }))
}

async fn dynamic_query(options: &Options<'_>, arguments: &JsonValue) -> Result<JsonValue, String> {
    let database = required_string_arg(arguments, "database")?;
    let auth = string_arg(arguments, "auth");
    let query_source = required_string_arg(arguments, "query")?;
    let input = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session_value = arguments
        .get("session")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let (database_schema, context, _paths) = current_schema_context(options, arguments)?;
    let query_list = pyre::parser::parse_query("mcp.pyre", &query_source)
        .map_err(|_| "Failed to parse dynamic Pyre query".to_string())?;
    let query_infos = pyre::typecheck::check_queries(&query_list, &context)
        .map_err(|errors| format!("Dynamic query failed typecheck: {errors:#?}"))?;

    let manifest = dynamic_manifest(&database_schema, &context, &query_list, &query_infos)?;
    let db = db::connect(&database, &auth)
        .await
        .map_err(|error| error.format_error())?;
    let conn = db.connect().map_err(|error| error.to_string())?;
    let session = PyreSession::new(session_value, &manifest.session_schema)
        .map_err(|error| error.to_string())?;

    let mut results = Vec::new();
    for query_def in &query_list.queries {
        if let ast::QueryDef::Query(query) = query_def {
            let result =
                pyre::server::query::run(&conn, &manifest, &query.name, input.clone(), &session)
                    .await
                    .map_err(|error| error.to_string())?;
            let manifest_query = manifest
                .queries
                .get(&query.name)
                .expect("manifest query exists");
            results.push(json!({
                "name": query.name,
                "operation": manifest_query.operation,
                "response": result.response,
                "affectedRows": result.affected_rows,
                "sql": manifest_query.sql
            }));
        }
    }

    Ok(json!({ "ok": true, "results": results }))
}

fn current_schema_context(
    options: &Options<'_>,
    arguments: &JsonValue,
) -> Result<
    (
        ast::Database,
        pyre::typecheck::Context,
        pyre::filesystem::Found,
    ),
    String,
> {
    let in_dir = input_dir(options, arguments);
    let paths = crate::filesystem::collect_filepaths(&in_dir).map_err(|error| error.to_string())?;
    let database_schema =
        super::shared::parse_database_schemas(&paths, false).map_err(|error| error.to_string())?;
    let context = pyre::typecheck::check_schema(&database_schema)
        .map_err(|errors| format!("Schema failed typecheck: {errors:#?}"))?;
    Ok((database_schema, context, paths))
}

fn dynamic_manifest(
    database: &ast::Database,
    context: &pyre::typecheck::Context,
    query_list: &ast::QueryList,
    query_infos: &HashMap<String, pyre::typecheck::QueryInfo>,
) -> Result<Manifest, String> {
    let mut queries = HashMap::new();
    for query_def in &query_list.queries {
        let ast::QueryDef::Query(query) = query_def else {
            continue;
        };
        let query_info = query_infos
            .get(&query.name)
            .ok_or_else(|| format!("Missing typecheck info for query {}", query.name))?;
        queries.insert(
            query.name.clone(),
            dynamic_query_manifest(context, query, query_info)?,
        );
    }

    Ok(Manifest {
        version: 1,
        session_schema: session_schema(database),
        queries,
    })
}

fn dynamic_query_manifest(
    context: &pyre::typecheck::Context,
    query: &ast::Query,
    query_info: &pyre::typecheck::QueryInfo,
) -> Result<QueryManifest, String> {
    let param_names = query_param_names(query, query_info);
    let mut sql = Vec::new();

    for field in &query.fields {
        let ast::TopLevelQueryField::Field(query_field) = field else {
            continue;
        };
        let table = context
            .tables
            .get(&query_field.name)
            .ok_or_else(|| format!("Unknown query table {}", query_field.name))?;
        for prepared in
            pyre::generate::sql::to_string(context, query, query_info, table, query_field)
        {
            sql.push(SqlInfo {
                include: prepared.include,
                params: param_names.clone(),
                sql: prepared.sql,
            });
        }
    }

    Ok(QueryManifest {
        id: query.name.clone(),
        operation: format!("{:?}", query.operation).to_lowercase(),
        primary_db: query_info.primary_db.clone(),
        attached_dbs: query_info.attached_dbs.iter().cloned().collect(),
        input_schema: input_schema(query_info),
        session_args: session_args(query_info),
        optional_input_args: query
            .args
            .iter()
            .filter(|arg| arg.omittable)
            .map(|arg| arg.name.clone())
            .collect(),
        json_input_args: Vec::new(),
        sql,
        sync_sql: None,
    })
}

fn input_schema(query_info: &pyre::typecheck::QueryInfo) -> HashMap<String, FieldSchema> {
    let mut schema = HashMap::new();
    for param in query_info.variables.values() {
        if let pyre::typecheck::ParamInfo::Defined {
            raw_variable_name,
            type_,
            nullable,
            from_session,
            ..
        } = param
        {
            if *from_session {
                continue;
            }
            schema.insert(
                raw_variable_name.clone(),
                FieldSchema {
                    type_: type_.clone().unwrap_or_else(|| "Json".to_string()),
                    nullable: *nullable,
                    omittable: false,
                },
            );
        }
    }
    schema
}

fn session_schema(database: &ast::Database) -> HashMap<String, FieldSchema> {
    let session = database
        .schemas
        .iter()
        .find_map(|schema| schema.session.clone())
        .unwrap_or_else(ast::default_session_details);
    let mut schema = HashMap::new();
    for field in session.fields {
        if let ast::Field::Column(column) = field {
            schema.insert(
                column.name,
                FieldSchema {
                    type_: column.type_.to_string(),
                    nullable: column.nullable,
                    omittable: false,
                },
            );
        }
    }
    schema
}

fn query_param_names(query: &ast::Query, query_info: &pyre::typecheck::QueryInfo) -> Vec<String> {
    let mut names = Vec::new();
    for param in query_info.variables.values() {
        if let pyre::typecheck::ParamInfo::Defined {
            raw_variable_name, ..
        } = param
        {
            names.push(raw_variable_name.clone());
        }
    }
    for arg in &query.args {
        if arg.omittable {
            names.push(format!("{}__is_set", arg.name));
        }
    }
    names.sort();
    names.dedup();
    names
}

fn session_args(query_info: &pyre::typecheck::QueryInfo) -> Vec<String> {
    let mut args = Vec::new();
    for param in query_info.variables.values() {
        if let pyre::typecheck::ParamInfo::Defined {
            from_session: true,
            session_name: Some(name),
            used: true,
            ..
        } = param
        {
            args.push(name.clone());
        }
    }
    args.sort();
    args.dedup();
    args
}

fn input_dir(options: &Options<'_>, arguments: &JsonValue) -> PathBuf {
    string_arg(arguments, "in")
        .map(PathBuf::from)
        .unwrap_or_else(|| options.in_dir.to_path_buf())
}

fn required_string_arg(arguments: &JsonValue, name: &str) -> Result<String, String> {
    string_arg(arguments, name).ok_or_else(|| format!("Missing required argument: {name}"))
}

fn string_arg(arguments: &JsonValue, name: &str) -> Option<String> {
    arguments
        .get(name)
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
}

fn bool_arg(arguments: &JsonValue, name: &str) -> Option<bool> {
    arguments.get(name).and_then(JsonValue::as_bool)
}

fn string_array_arg(arguments: &JsonValue, name: &str) -> Result<Vec<String>, String> {
    match arguments.get(name) {
        None | Some(JsonValue::Null) => Ok(Vec::new()),
        Some(JsonValue::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(ToString::to_string)
                    .ok_or_else(|| format!("{name} must be an array of strings"))
            })
            .collect(),
        _ => Err(format!("{name} must be an array of strings")),
    }
}

fn push_optional_arg(arguments: &JsonValue, args: &mut Vec<String>, name: &str, flag: &str) {
    if let Some(value) = string_arg(arguments, name) {
        args.push(flag.to_string());
        args.push(value);
    }
}

fn sorted_keys<T>(map: &HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

fn tools() -> Vec<JsonValue> {
    vec![
        tool("pyre_project_info", "Report basic information about the current Pyre project.", json!({"type":"object","properties":{"in":{"type":"string"},"generated":{"type":"string"},"migration_dir":{"type":"string"}}})),
        tool("pyre_docs", "Return bundled Pyre documentation by topic.", json!({"type":"object","properties":{"topic":{"type":"string","enum":["overview","docs","usage","dev","cli"]}}})),
        tool("pyre_schema", "Return all Pyre schema files and their contents.", json!({"type":"object","properties":{"in":{"type":"string"}}})),
        tool("pyre_check", "Typecheck the current Pyre schema and query files.", json!({"type":"object","properties":{"in":{"type":"string"}}})),
        tool("pyre_format", "Format Pyre files. This may write files unless to_stdout is true.", json!({"type":"object","properties":{"in":{"type":"string"},"files":{"type":"array","items":{"type":"string"}},"to_stdout":{"type":"boolean"}}})),
        tool("pyre_generate", "Generate Pyre artifacts.", json!({"type":"object","properties":{"in":{"type":"string"},"out":{"type":"string"}}})),
        tool("pyre_generate_migration", "Generate a Pyre migration against a database.", json!({"type":"object","required":["name","database"],"properties":{"in":{"type":"string"},"name":{"type":"string"},"database":{"type":"string"},"auth":{"type":"string"},"namespace":{"type":"string"},"migration_dir":{"type":"string"}}})),
        tool("pyre_migrate", "Apply Pyre migrations to a database. This can modify the database.", json!({"type":"object","required":["database"],"properties":{"in":{"type":"string"},"database":{"type":"string"},"auth":{"type":"string"},"namespace":{"type":"string"},"migration_dir":{"type":"string"},"push":{"type":"boolean"}}})),
        tool("pyre_db_status", "Check database connectivity and current schema/migration status.", json!({"type":"object","required":["database"],"properties":{"in":{"type":"string"},"database":{"type":"string"},"auth":{"type":"string"},"namespace":{"type":"string"},"migration_dir":{"type":"string"}}})),
        tool("pyre_query", "Typecheck and execute raw dynamic Pyre query text against a database. Supports reads and mutations.", json!({"type":"object","required":["database","query"],"properties":{"in":{"type":"string"},"database":{"type":"string"},"auth":{"type":"string"},"query":{"type":"string"},"params":{"type":"object"},"session":{"type":"object"}}})),
    ]
}

fn tool(name: &str, description: &str, input_schema: JsonValue) -> JsonValue {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn success_response(id: JsonValue, result: JsonValue) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: JsonValue, code: i64, message: &str) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn parse_error(message: String) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": -32700,
            "message": message
        }
    })
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }

        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                let length = value.trim().parse::<usize>().map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
                })?;
                content_length = Some(length);
            }
        }
    }

    let length = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;

    let mut message = vec![0; length];
    reader.read_exact(&mut message)?;
    Ok(Some(message))
}

fn write_message<W: Write>(writer: &mut W, message: &JsonValue) -> io::Result<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
