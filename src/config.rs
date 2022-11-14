use crate::args::Args;
use anyhow::{bail, Result};
use log::debug;
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub level: Option<usize>,
    pub err_level: Option<usize>,
    pub string: Option<String>,
    pub err_string: Option<String>,
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(config) = toml::from_str(&contents) {
                Ok(config)
            } else {
                bail!("Malformatted config file '{}'", path.display());
            }
        } else {
            bail!(
                "Unable to open config file '{}'. Does it exist and is permission allowed?",
                path.display()
            );
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            level: Some(2),
            err_level: None, // Note that even though 'err_level', when unset, follows 'level' it is unset by default. We will later set it if it is still unset
            string: None,
            err_string: None,
        }
    }
}

pub fn apply_args_to_config(config: &mut Config, args: &Args) -> Result<()> {
    match (args.level, &args.string) {
        (Some(level), None) => {
            config.level = Some(level);
            if config.string.is_some() {
                debug!("User overrode config file value 'string' in favor of argument '--level'");
                config.string = None;
            }
        }
        (None, Some(string)) => {
            config.string = Some(string.to_string());
            if config.level.is_some() {
                debug!("User overrode config file value 'level' in favor of argument '--string'");
                config.level = None;
            }
        }
        (None, None) => {
            // Nothing to do
        }
        _ => unreachable!(
            "Mutually exclusive command line args '--level' and '--string' were both set???"
        ),
    }

    match (args.err_level, &args.err_string) {
        (Some(err_level), None) => {
            config.err_level = Some(err_level);
            if config.err_string.is_some() {
                debug!("User overrode config file value 'err_string' in favor of argument '--err-level'");
                config.err_string = None;
            }
        },
        (None, Some(err_string)) => {
            config.err_string = Some(err_string.to_string());
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

    Ok(())
}
