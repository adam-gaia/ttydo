#![allow(unused)] // TODO: remove
use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, ArgGroup};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::io::AsyncRead;
use tokio::io::Lines;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::{Stream, StreamExt, StreamMap};
use which::which;
use directories_next::ProjectDirs;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub level: Option<usize>,
    pub err_level: Option<usize>,
    pub string: Option<String>,
    pub err_string: Option<String>
}

impl Config {
    fn from_file(path: &Path) -> Result<Self> {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(config) = toml::from_str(&contents) {
                Ok(config)
            }
            else {
                bail!("Malformatted config file '{}'", path.display());
            }
        }
        else {
            bail!("Unable to open config file '{}'. Does it exist and is permission allowed?", path.display());
        }
    }
}

impl Default for Config {
    fn default() -> Self { Config {
            level: Some(2),
            err_level: None, // Note that even though 'err_level', when unset, follows 'level' it is unset by default. We will later set it if it is still unset
            string: None,
            err_string: None
        }
    }
}

/// Run a command and indent its output
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(
            ArgGroup::new("stdout_indentation")
                .multiple(false)
                .args(["level", "string"]),
        ))]
#[command(group(
            ArgGroup::new("stderr_indentation")
                .multiple(false)
                .args(["err_level", "err_string"]),
        ))]
struct Args {
    /// Use an alternate config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Level of indentation
    #[arg(short, long)]
    level: Option<usize>,

    /// Level of indentation for stderr - defaults to level set for stdout
    #[arg(long)]
    err_level: Option<usize>,

    /// Custom indentation string
    #[arg(short, long)]
    string: Option<String>,

    /// Custom indentation string for stderr stream - defaults to string set for stdout
    #[arg(long)]
    err_string: Option<String>,

    // Command to run and indent
    #[arg(last = true)]
    command: Vec<String>,
}

struct App {
    stdout_ind: String,
    stderr_ind: String,
}

impl App {
    fn from_config(config: &Config) -> Result<Self> { 
        let stdout_ind = if let Some(level) = config.level {
            " ".repeat(level)
        } else if let Some(string) = &config.string {
            string.to_string()
        } else {
            unreachable!("Cannot set both 'level' and 'string' config/arg values");
        };
        let stderr_ind = if let Some(err_level) = config.err_level {
            " ".repeat(err_level)
        } else if let Some(err_string) = &config.err_string {
            err_string.to_string()
        } else {
            unreachable!("Cannot set both 'level' and 'string' config/arg values");
        };
        let app = App { stdout_ind, stderr_ind };
        Ok(app)
    }

    async fn indent_stdout(&self) {}
    async fn indent_stderr(&self) {}

    async fn run_command(&self, command: &[String]) -> Result<(ExitStatus)> {
        if let Some((exec, args)) = command.split_first() {
            if let Err(..) = which(exec) {
                bail!("Unable to find '{}' on the system path", exec);
            }

            let mut child = Command::new(exec)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            // TODO: handle None case instead of unwrapping
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let mut stdout_reader = LinesStream::new(BufReader::new(stdout).lines());
            let mut stderr_reader = LinesStream::new(BufReader::new(stderr).lines());

            let a = Box::pin(async_stream::stream! {
                while let Some(item) = stdout_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = std::result::Result<String, std::io::Error>> + Send>>;
            let b = Box::pin(async_stream::stream! {
                while let Some(item) = stderr_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = std::result::Result<String, std::io::Error>> + Send>>;

            let mut map = StreamMap::with_capacity(2);
            map.insert("stdout", a);
            map.insert("stderr", b);

            debug!("Running process '{}' with args {:?}", exec, args);
            let handle: tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>> =
                tokio::spawn(async move { child.wait().await });

            // Stream output to terminal as it runs
            while let Some((source, line)) = map.next().await {
                let line = line?;
                let indent = match source {
                    "stdout" => {
                        &self.stdout_ind
                    },
                    "stderr" => {
                        &self.stderr_ind
                    },
                    _ => {
                        unreachable!("Stream must be stdout or stder");
                    }
                };
                println!("{}{}", indent, line);
            }

            let child_status = handle.await??;
            debug!("Child process exited with status {}", child_status);
            if !child_status.success() {
                // TODO figure out rust's ExitStatusExt to check if a
                // signal killed our process and remove the unwrap
                // TODO: better error handeling to report if a pre/post/override/originalcmd failed
                bail!(
                    "'{} {:?}' returned non-zero exit code {}",
                    exec,
                    args,
                    child_status.code().unwrap()
                );
            }
            Ok(child_status)

        }
        else {
            bail!("todo: better err message");
        } 
    }
}

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
    let mut config = if let Some(path) = args.config {
        Config::from_file(&path)? // Propigate error
    }
    else if let Some(path) = standard_config_file(this_program_name) {
        if let Ok(config) = Config::from_file(&path) {
            config
        }
        else {
            Config::default()
        }
    }
    else {
        Config::default()
    };

    match (args.level, args.string) {
        (Some(level), None) => {
            config.level = Some(level);
            if config.string.is_some() {
                debug!("User overrode config file value 'string' in favor of argument '--level'");
                config.string = None;
            }
        },
        (None, Some(string)) => {
            config.string = Some(string);
            if config.level.is_some() {
                debug!("User overrode config file value 'level' in favor of argument '--string'");
                config.level = None;
            }
        },
        (None, None) => {
            // Nothing to do
        },
        _ => unreachable!("Mutually exclusive command line args '--level' and '--string' were both set???")
    }

    match (args.err_level, args.err_string) {
        (Some(err_level), None) => {
            config.err_level = Some(err_level);
            if config.err_string.is_some() {
                debug!("User overrode config file value 'err_string' in favor of argument '--err-level'");
                config.err_string = None;
            }
        },
        (None, Some(err_string)) => {
            config.err_string = Some(err_string);
            if config.err_level.is_some() {
                debug!("User overrode config file value 'err_level' in favor of argument '--err-string'");
                config.err_level = None;
            }
        },
        (None, None) => {
            // Nothing to do
        },
        _ => unreachable!("Mutually exclusive command line args '--err_level' and '--err_string' were both set???")
    }

    // When the stderr level/string is not explicitly set it follows the stdout
    if config.err_level.is_none() && config.err_string.is_none() {
        // level/string
        if config.level.is_some() {
            config.err_level = config.level;
        }
        if config.string.is_some() {
            config.err_string = config.string.clone();
        }
    }

    debug!("Config: {:?}", &config);

    let app = App::from_config(&config)?;
    let command = args.command;
    app.run_command(&command).await?; 
    Ok(())
}
