use crate::config::Config;
use crate::xcommand::{StdioType, XCommand};
use anyhow::{bail, Result};
use futures_util::pin_mut;
use log::debug;
use tokio_stream::StreamExt;

pub struct App<'a> {
    stdout_ind: String,
    stderr_ind: String,
    command: &'a [String],
}

impl<'a> App<'a> {
    pub fn with_default_base(command: &'a [String]) -> Result<Self> {
        Ok(App {
            command,
            stdout_ind: String::from("  "),
            stderr_ind: String::from("  "),
        })
    }

    pub fn from_config(config: &Config, command: &'a [String]) -> Result<Self> {
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
        Ok(App {
            command,
            stdout_ind,
            stderr_ind,
        })
    }
    pub async fn run(&self) -> Result<i32> {
        let Some((exec, exec_args)) = self.command.split_first() else {
            bail!("Invaid input command");
        };
        let cmd = XCommand::new(exec, exec_args);
        let child = cmd.spawn().await.unwrap();
        debug!("Child pid: {}", child.pid());

        let s = child.stream();
        pin_mut!(s);
        while let Some(output) = s.next().await {
            let (source, line) = output.unwrap();
            match source {
                StdioType::Stdout => {
                    println!("{}{}", self.stdout_ind, line);
                }
                StdioType::Stderr => {
                    eprintln!("{}{}", self.stderr_ind, line);
                }
            }
        }
        Ok(0)
    }
}
