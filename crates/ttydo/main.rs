#![feature(async_iterator)]
use anyhow::{bail, Result};
use clap::Parser;
use log::debug;

use commandstream::CommandStream;

/// Run a command with a fake tty
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Command to run
    #[arg(last = true)]
    command: Vec<String>,
}

struct TTYCommand<'a> {
    command: &'a [String],
}
impl<'a> TTYCommand<'a> {
    fn new(command: &'a [String]) -> Result<Self> {
        Ok(TTYCommand { command })
    }
}

impl<'a> CommandStream<'_> for TTYCommand<'a> {
    fn command(&self) -> &[String] {
        &self.command
    }

    fn handle_stdout(&self, line: &str) -> Result<()> {
        println!("{}", line);
        Ok(())
    }

    fn handle_stderr(&self, line: &str) -> Result<()> {
        eprintln!("{}", line);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let command = args.command;
    if command.is_empty() {
        bail!("Please specify a command to run following '--'");
    }
    let app = TTYCommand::new(&command)?;
    debug!("Running");
    let return_code = app.run().await?;
    debug!("Done");
    std::process::exit(return_code);
}
