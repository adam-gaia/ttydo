use anyhow::{bail, Result};
use async_trait::async_trait;
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
