use clap::{Parser, Subcommand};
use std::io::{self};
use std::path::Path;

mod command;
mod db;
mod filesystem;

#[derive(Parser)]
#[command(name = "pyre")]
#[command(about = "A CLI tool for pyre operations", long_about = None)]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The input directory to read from.
    #[arg(long, global = true, default_value = "pyre")]
    r#in: String,

    #[arg(long, global = true)]
    version: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Get started using Pyre.  Generates a starter schema.
    Init {
        /// Generate a setup that has multiple database schemas.
        #[arg(long, default_value_t = false)]
        multidb: bool,
    },

    /// Generate files for querying your pyre schema.
    Generate {
        /// Directory where output files will be written.
        #[arg(long, default_value = "pyre/generated")]
        out: String,
    },

    /// Format files
    Format {
        #[arg(required = false)]
        files: Vec<String>,

        /// Output to stdout instead of files
        #[arg(long, default_value_t = false)]
        to_stdout: bool,
    },

    /// Typecheck your schema and queries.
    Check {
        #[arg(required = false)]
        files: Vec<String>,

        /// Format errors as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Introspect a database and generate a pyre schema.
    Introspect {
        /// A local filename, or a url, or an environment variable if prefixed with a $.
        database: String,

        /// The Pyre namespace to store this schema under.
        #[arg(long)]
        namespace: Option<String>,

        #[arg(long)]
        auth: Option<String>,
    },

    /// Execute any migrations that are needed.
    Migrate {
        /// A local filename, or a url, or an environment variable if prefixed with a $.
        database: String,

        #[arg(long)]
        auth: Option<String>,

        /// The Pyre schema to migrate
        #[arg(long)]
        namespace: Option<String>,

        /// Push changes directly to the DB
        #[arg(long, default_value_t = false)]
        push: bool,

        /// Directory where migration files are stored.
        #[arg(long, default_value = "pyre/migrations")]
        migration_dir: String,
    },

    /// Create new resources
    New {
        #[command(subcommand)]
        resource: NewCommands,
    },
}

#[derive(Subcommand)]
enum NewCommands {
    /// Generate a migration
    Migration {
        /// The migration name.
        name: String,

        #[arg(long)]
        db: String,

        #[arg(long)]
        auth: Option<String>,

        /// The Pyre namespace to generate a migration for.
        #[arg(long)]
        namespace: Option<String>,

        /// Directory where migration files are stored.
        #[arg(long, default_value = "pyre/migrations")]
        migration_dir: String,
    },
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if let true = cli.version {
        println!("0.1.0");
        return Ok(());
    }

    // Check if stderr is a TTY to determine if we should enable color output.
    // Disable color when output is redirected (e.g., `pyre check 2> errors.txt`)
    // or piped (e.g., `pyre check 2>&1 | grep error`) to avoid ANSI codes in files/pipes.
    let enable_color = atty::is(atty::Stream::Stderr);
    let options = command::Options {
        in_dir: Path::new(&cli.r#in),
        enable_color,
    };

    match &cli.command {
        Commands::Init { multidb } => {
            command::init(&options, *multidb)?;
        }
        Commands::Generate { out } => {
            command::generate(&options, out)?;
        }
        Commands::Format { files, to_stdout } => {
            command::format(&options, files, *to_stdout)?;
        }
        Commands::Check { files, json } => {
            command::check(&options, files.clone(), *json)?;
        }
        Commands::Introspect {
            database,
            auth,
            namespace,
        } => {
            command::introspect(&options, database, auth, namespace).await?;
        }
        Commands::Migrate {
            database,
            auth,
            push,
            migration_dir,
            namespace,
        } => {
            if *push {
                command::push(&options, database, auth, namespace).await?;
            } else {
                command::migrate(&options, database, auth, migration_dir, namespace).await?;
            }
        }
        Commands::New { resource } => match resource {
            NewCommands::Migration {
                name,
                db,
                auth,
                migration_dir,
                namespace,
            } => {
                command::generate_migration(
                    &options,
                    name,
                    db,
                    auth,
                    Path::new(migration_dir),
                    namespace,
                )
                .await?;
            }
        },
    }
    Ok(())
}
