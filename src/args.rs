use clap::{ArgGroup, Parser};
use std::path::PathBuf;

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
pub struct Args {
    /// Use an alternate config file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Level of indentation
    #[arg(short, long)]
    pub level: Option<usize>,

    /// Level of indentation for stderr - defaults to level set for stdout
    #[arg(long)]
    pub err_level: Option<usize>,

    /// Custom indentation string
    #[arg(short, long)]
    pub string: Option<String>,

    /// Custom indentation string for stderr stream - defaults to string set for stdout
    #[arg(long)]
    pub err_string: Option<String>,

    // Command to run and indent
    #[arg(last = true)]
    pub command: Vec<String>,
}
