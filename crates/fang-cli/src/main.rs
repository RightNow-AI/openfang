use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "fang",
    about = "The FangHub CLI — publish and manage OpenFang Hand packages",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with FangHub using a GitHub token
    Login,
    /// Log out from FangHub
    Logout,
    /// Package the current directory as a Hand archive
    Package {
        /// Output path for the .tar.gz archive (default: ./<package_id>-<version>.tar.gz)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Publish the current Hand package to FangHub
    Publish {
        /// FangHub registry URL (default: https://fanghub.paradiseai.io)
        #[arg(long, default_value = "https://fanghub.paradiseai.io")]
        registry: String,
        /// Release notes for this version
        #[arg(long)]
        notes: Option<String>,
    },
    /// Search for Hand packages on FangHub
    Search {
        /// Search query
        query: String,
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
    },
    /// Show information about a Hand package
    Info {
        /// Package ID (e.g. "my-weather-hand")
        package_id: String,
    },
    /// Install a Hand package from FangHub into the local OpenFang instance
    Install {
        /// Package ID and optional version (e.g. "my-weather-hand" or "my-weather-hand@1.2.3")
        package: String,
        /// OpenFang API URL (default: http://localhost:3000)
        #[arg(long, default_value = "http://localhost:3000")]
        api: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Login => fang_cli::commands::login::run().await,
        Commands::Logout => fang_cli::commands::logout::run().await,
        Commands::Package { output } => fang_cli::commands::package::run(output).await,
        Commands::Publish { registry, notes } => {
            fang_cli::commands::publish::run(&registry, notes).await
        }
        Commands::Search { query, category } => {
            fang_cli::commands::search::run(&query, category).await
        }
        Commands::Info { package_id } => fang_cli::commands::info::run(&package_id).await,
        Commands::Install { package, api } => {
            fang_cli::commands::install::run(&package, &api).await
        }
    }
}
