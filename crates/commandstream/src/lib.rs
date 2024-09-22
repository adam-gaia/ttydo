use async_trait::async_trait;
use color_eyre::{bail, Result};
use futures_util::pin_mut;
use log::debug;
use tokio_stream::StreamExt;
use ttycommand::{StdioType, TTYCommand};

#[async_trait]
pub trait CommandStream<'a> {
    fn command(&self) -> &[String];

    fn handle_stdout(&self, line: &str) -> Result<()>;
    fn handle_stderr(&self, line: &str) -> Result<()>;

    async fn run(&self) -> Result<i32> {
        let Some((exec, exec_args)) = self.command().split_first() else {
            bail!("Invaid input command");
        };
        let cmd = TTYCommand::new(exec, exec_args);
        let child = cmd.spawn().await.unwrap();
        debug!("Child pid: {}", child.pid());

        let s = child.stream();
        pin_mut!(s);
        while let Some(output) = s.next().await {
            let (source, line) = output.unwrap();
            match source {
                StdioType::Stdout => {
                    self.handle_stdout(&line)?;
                }
                StdioType::Stderr => {
                    self.handle_stderr(&line)?;
                }
            }
        }
        // TODO: why arent we returning the child's return code here?
        Ok(0)
    }
}

pub struct SimpleCommand<'a> {
    command: &'a [String],
}
impl<'a> SimpleCommand<'a> {
    pub fn new(command: &'a [String]) -> Result<Self> {
        Ok(SimpleCommand { command })
    }
}

impl<'a> CommandStream<'_> for SimpleCommand<'a> {
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
