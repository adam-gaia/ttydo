#![feature(async_iterator)]
use clap::Parser;
use color_eyre::{bail, Result};
use log::debug;

use commandstream::{CommandStream, SimpleCommand};

/// Run a command with a fake tty
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Command to run
    #[arg(last = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let command = args.command;
    if command.is_empty() {
        bail!("Please specify a command to run following '--'");
    }
    let app = SimpleCommand::new(&command)?;
    debug!("Running");
    let return_code = app.run().await?;
    debug!("Done");
    std::process::exit(return_code);
}
