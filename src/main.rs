#![feature(async_iterator)]
use anyhow::{bail, Result};
use clap::Parser;
use directories_next::ProjectDirs;
use log::debug;
use std::path::PathBuf;

mod config;
use config::{apply_args_to_config, Config};

mod app;
use app::App;

mod args;
use args::Args;

mod xcommand;

// TODO: only run subprocess under tty if this program was run under a tty

/// Return standard config file path if it exists (XDG_CONFIG_DIR/<program_name>/config.toml)
fn standard_config_file(this_program_name: &str) -> Option<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("", "", this_program_name) {
        let mut config_file = proj_dirs.config_dir().to_owned();
        config_file.push("config.toml");
        if config_file.is_file() {
            return Some(config_file);
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let this_program_name = clap::crate_name!();
    let args = Args::parse();

    // First check if the user specified a config file
    // Otherwise, fall back to the standard config file if it exists
    // If all else fails, use defaults
    let mut config = if let Some(path) = &args.config {
        Config::from_file(path)? // Propigate error
    } else if let Some(path) = standard_config_file(this_program_name) {
        if let Ok(config) = Config::from_file(&path) {
            config
        } else {
            Config::default()
        }
    } else {
        Config::default()
    };

    apply_args_to_config(&mut config, &args);
    debug!("Config: {:?}", &config);

    let command = args.command;
    if command.is_empty() {
        bail!("Please specify a command to run following '--'");
    }

    debug!("Building");
    let indenter = App::from_config(&config, &command)?;
    debug!("Running");
    let return_code = indenter.run().await?;
    debug!("Done");
    Ok(())
}
