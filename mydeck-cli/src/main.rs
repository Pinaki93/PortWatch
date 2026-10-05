use clap::{Parser, Subcommand};
use loco_rs::{
    app::Hooks,
    boot::{create_app, create_context, start, ServeParams, StartMode},
    environment::{resolve_from_env, Environment},
    logger,
};
use port_watch::app::App;
use std::path::Path;

#[derive(Debug, Parser)]
#[command(name = "mydeck", version, about = "Run MyDeck")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the PortWatch server
    Serve {
        /// Override the configured server port
        #[arg(short, long)]
        port: Option<u16>,
    },
}

#[tokio::main]
async fn main() -> loco_rs::Result<()> {
    let Cli {
        command: Command::Serve { port },
    } = Cli::parse();

    let environment: Environment = resolve_from_env().into();
    // ponytail: config stays in this checkout; embed defaults for standalone distribution.
    let config =
        environment.load_from_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../config"))?;
    let app_context = create_context::<App>(&environment, config).await?;

    if !App::init_logger(&app_context)? {
        logger::init::<App>(&app_context.config.logger)?;
    }

    let boot = create_app::<App>(StartMode::ServerOnly, &environment, app_context.config).await?;
    let serve_params = ServeParams {
        port: port.map_or(boot.app_context.config.server.port, i32::from),
        binding: boot.app_context.config.server.binding.clone(),
    };

    start::<App>(boot, serve_params, false).await
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn parses_serve_port() {
        let cli = Cli::try_parse_from(["mydeck", "serve", "--port", "5151"]).unwrap();

        assert!(matches!(cli.command, Command::Serve { port: Some(5151) }));
    }
}
