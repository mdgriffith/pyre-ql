use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pyre::ast;
use pyre::db::diff::to_sql;
use pyre::db::introspect;
use pyre::parser;
use pyre::typecheck;

const TEST_SCHEMA: &str = r#"
record User {
    accounts      @link(Account.userId)
    posts         @link(Post.authorUserId)
    databaseUsers @link(DatabaseUser.userId)

    // Fields
    id        Int     @id
    name      String?
    status    Status
    createdAt DateTime @default(now)
}

record DatabaseUser {
    id         Int   @id
    databaseId String

    userId Int
    users  @link(userId, User.id)
}

record Account {
    @tablename "accounts"
    users @link(userId, User.id)

    id     Int   @id
    userId Int
    name   String
    status String
}

record Post {
    users @link(authorUserId, User.id)

    id           Int     @id
    createdAt    DateTime @default(now)
    authorUserId Int
    title        String
    content      String
    status       Status
}

type Status
   = Active
   | Inactive
   | Special {
        reason String
     }
   | Special2 {
        reason String
        error  String
     }
"#;

fn parser_benchmark(c: &mut Criterion) {
    c.bench_function("parser::run", |b| {
        b.iter(|| {
            let mut schema = ast::Schema::default();
            parser::run("schema.pyre", black_box(TEST_SCHEMA), &mut schema).unwrap();
        })
    });
}

fn typecheck_benchmark(c: &mut Criterion) {
    c.bench_function("typecheck::check_schema", |b| {
        b.iter(|| {
            let mut schema = ast::Schema::default();
            parser::run("schema.pyre", TEST_SCHEMA, &mut schema).unwrap();
            let database = ast::Database {
                schemas: vec![schema],
            };
            typecheck::check_schema(&database).unwrap();
        })
    });
}

fn diff_schema_benchmark(c: &mut Criterion) {
    c.bench_function("diff::diff_schema", |b| {
        b.iter(|| {
            let mut old_schema = ast::Schema::default();
            parser::run("schema.pyre", TEST_SCHEMA, &mut old_schema).unwrap();
            let new_schema = old_schema.clone();
            pyre::ast::diff::diff_schema(&old_schema, &new_schema);
        })
    });
}

fn db_diff_benchmark(c: &mut Criterion) {
    c.bench_function("db::diff::diff", |b| {
        b.iter(|| {
            let mut schema = ast::Schema::default();
            parser::run("schema.pyre", TEST_SCHEMA, &mut schema).unwrap();
            let database = ast::Database {
                schemas: vec![schema],
            };
            let context = typecheck::check_schema(&database).unwrap();

            // Create a minimal introspection for testing
            let introspection = introspect::Introspection {
                tables: vec![],
                migration_state: introspect::MigrationState::NoMigrationTable,
                schema: introspect::SchemaResult::Success {
                    schema: ast::Schema::default(),
                    context: typecheck::empty_context(),
                },
            };

            pyre::db::diff::diff(&context, &database.schemas[0], &introspection);
        })
    });
}

fn to_sql_benchmark(c: &mut Criterion) {
    c.bench_function("db::diff::to_sql::to_sql", |b| {
        b.iter(|| {
            let mut schema = ast::Schema::default();
            parser::run("schema.pyre", TEST_SCHEMA, &mut schema).unwrap();
            let database = ast::Database {
                schemas: vec![schema],
            };
            let context = typecheck::check_schema(&database).unwrap();

            // Create a minimal introspection for testing
            let introspection = introspect::Introspection {
                tables: vec![],
                migration_state: introspect::MigrationState::NoMigrationTable,
                schema: introspect::SchemaResult::Success {
                    schema: ast::Schema::default(),
                    context: typecheck::empty_context(),
                },
            };

            let diff = pyre::db::diff::diff(&context, &database.schemas[0], &introspection);
            to_sql::to_sql(&diff);
        })
    });
}

criterion_group!(
    benches,
    parser_benchmark,
    typecheck_benchmark,
    diff_schema_benchmark,
    db_diff_benchmark,
    to_sql_benchmark
);
criterion_main!(benches);
