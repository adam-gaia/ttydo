use async_stream::stream;
use color_eyre::{eyre::bail, Result};
use log::debug;
use nix::pty::openpty;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::dup2;
use nix::unistd::execve;
use nix::unistd::ForkResult;
use nix::unistd::{close, fork, Pid};
use std::env;
use std::ffi::CString;
use std::os::fd::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::path::Path;
use std::pin::Pin;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio_fd::AsyncFd;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::{Stream, StreamExt, StreamMap};
use which::which;

/// Anyhow-ify the result type that CString::new() returns
fn str2cstring(s: &str) -> Result<CString> {
    if let Ok(c) = CString::new(s) {
        return Ok(c);
    }
    bail!("Unable to convert str '{}' to cstring", s);
}

fn vec_slice_of_string_2_vec_of_cstring(input: &[String]) -> Result<Vec<CString>> {
    // TODO: Handle this unwrap!
    let output = input.iter().map(|s| str2cstring(s).unwrap()).collect();
    Ok(output)
}

/// Convert a Path to CString
fn path2cstring(p: &Path) -> Result<CString> {
    if let Some(s) = p.to_str() {
        if let Ok(c) = str2cstring(s) {
            return Ok(c);
        };
    };
    bail!("Unable to convert pathbuf '{}' to str", p.display());
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum StdioType {
    Stdout,
    Stderr,
}

pub enum XStatus {
    Exited(i32),
    Signaled(Signal),
}

// TODO: instead of types like 'XChildHandle' and 'XStatus' it would be nice to return Tokio's
// 'ChildHandle' and 'Status' for api compat with tokio::command and std::command
pub struct XChildHandle {
    pid: Pid,
    stdout_raw_fd: RawFd,
    stderr_raw_fd: RawFd,
}

impl XChildHandle {
    fn new(pid: Pid, stdout_raw_fd: RawFd, stderr_raw_fd: RawFd) -> Result<Self> {
        Ok(XChildHandle {
            pid,
            stdout_raw_fd,
            stderr_raw_fd,
        })
    }
    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn stream(&self) -> impl Stream<Item = Result<(StdioType, String), std::io::Error>> + '_ {
        stream! {
            let child_pid = self.pid;

            let mut join = tokio::task::spawn_blocking(move || {
                // TODO: I think we can use a regular spawn instead of a non-blocking spawn if we pass
                // 'WaitPidFlag::WNOHANG' to waitpid() and loop over waitpid() calls, continuing when
                // we match WaitStatus::StillAlive
                // https://docs.rs/nix/latest/nix/sys/wait/enum.WaitStatus.html
                    let Ok(status) = waitpid(child_pid, None) else {
                        panic!("Error waiting for child to complete");
                    };
                    debug!("Child status: {:?}", status);
                    status
            });

            let stdout = AsyncFd::try_from(self.stdout_raw_fd).unwrap();
            let stderr = AsyncFd::try_from(self.stderr_raw_fd).unwrap();

            let mut stdout_reader = LinesStream::new(BufReader::new(stdout).lines());
            let mut stderr_reader = LinesStream::new(BufReader::new(stderr).lines());

            let stdout_stream = Box::pin(stream! {
                while let Some(Ok(item)) = stdout_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = String> + Send>>;

            let stderr_stream = Box::pin(stream! {
                while let Some(Ok(item)) = stderr_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = String> + Send>>;

            let mut map = StreamMap::with_capacity(2);
            map.insert(StdioType::Stdout, stdout_stream);
            map.insert(StdioType::Stderr, stderr_stream);

            loop {
                tokio::select! {
                    // Force polling in listed order instead of randomly. This prevents us from
                    // deadlocking when the command exits. - TODO: this might not be needed anymore
                    biased;
                    Some(output) = map.next() => {
                        yield Ok(output);
                    },
                    status = &mut join => {
                        debug!("status");

                        let status = status.unwrap(); // TODO: handle unwrap

                        // Pick up any final output that was written in the time it took us to check
                        // this 'select!' branch
                        while let Some(output) = map.next().await {
                            yield Ok(output);
                        }

                        // Clean up by closing file descriptors given to us by openpty()
                        close(self.stdout_raw_fd).unwrap();
                        close(self.stderr_raw_fd).unwrap();

                        // TODO: do we need to handle any other WaitStatus varients?
                        // https://docs.rs/nix/latest/nix/sys/wait/enum.WaitStatus.html
                        match status {
                            WaitStatus::Exited(pid, return_code) => {
                                debug!("Child exited with return code {}", return_code);
                                //return Ok(XStatus::Exited(return_code))
                                // TODO: how do we return the child's status?
                                return;
                            },
                            WaitStatus::Signaled(pid, signal, _) => {
                                debug!("Child was killed by signal {:?}", signal);
                                //return Ok(XStatus::Signaled(signal))
                                return;
                            },
                            _ => {
                                panic!("Child process in unexpected state: '{:?}'", status);
                            },
                        }
                    },
                }
            }
        }
    }
}

pub struct TTYCommand<'a> {
    command: &'a str,
    args: &'a [String],
    env: Vec<String>,
}
impl<'a> TTYCommand<'a> {
    pub fn new(command: &'a str, args: &'a [String]) -> Self {
        // Format env vars in a list of "key=value"
        let env: Vec<String> = env::vars()
            .into_iter()
            .map(|x| format!("{}={}", x.0, x.1))
            .collect();
        TTYCommand { command, args, env }
    }

    /// Replace the current process with the executed command
    async fn exec(&self) -> Result<()> {
        let Ok(command) = which(self.command) else {
            bail!("Unable to find '{}' on path", self.command);
        };

        // Prepend the comnand name to the array of args
        let mut fixed_args = Vec::new();
        fixed_args.push(command.to_str().unwrap().to_owned());
        fixed_args.extend_from_slice(self.args);

        let command = path2cstring(&command).unwrap();
        let args = vec_slice_of_string_2_vec_of_cstring(&fixed_args).unwrap();
        let env = vec_slice_of_string_2_vec_of_cstring(&self.env).unwrap();

        // Cannot call println or unwrap in child - see
        // https://docs.rs/nix/0.25.0/nix/unistd/fn.fork.html#safety
        //nix::unistd::write(libc::STDOUT_FILENO, "I'm a new child process - stdout\n".as_bytes()).ok();
        //nix::unistd::write(libc::STDERR_FILENO, "I'm a new child process - stderr\n".as_bytes()).ok();

        if execve(&command, &args, &env).is_err() {
            bail!(
                "Unable to execve command '{:?}' with args {:?}",
                command,
                args
            );
        }
        Ok(())
    }

    pub async fn spawn(&self) -> Result<XChildHandle> {
        // Open two ptys, one for stdout and one for stderr
        // This seems ludicrous howerver I cannot find a way to seprately send both streams and
        // fake a pty.
        // This SO question summs it up
        // https://stackoverflow.com/questions/34186035/can-you-fool-isatty-and-log-stdout-and-stderr-separately
        let Ok(stdout_pty) = openpty(None, None) else {
            bail!("Unable to create pty for stdout");
        };
        let stdout_read_side = stdout_pty.master;
        let stdout_write_side = stdout_pty.slave;

        let Ok(stderr_pty) = openpty(None, None) else {
            bail!("Unable to create pty for stderr");
        };
        let stderr_read_side = stderr_pty.master;
        let stderr_write_side = stderr_pty.slave;

        let Ok(res) = (unsafe { fork() }) else {
            bail!("fork() failed");
        };
        match res {
            ForkResult::Parent { child } => {
                // We are the parent
                close(stdout_write_side.as_raw_fd()).unwrap();
                close(stderr_write_side.as_raw_fd()).unwrap();

                // Return a handle to the child
                Ok(XChildHandle::new(
                    child,
                    stdout_read_side.as_raw_fd(),
                    stderr_read_side.as_raw_fd(),
                )
                .unwrap())
            }
            ForkResult::Child => {
                // We are the child
                close(stdout_read_side.as_raw_fd()).unwrap();
                close(stderr_read_side.as_raw_fd()).unwrap();

                // Redirect stdout/err to pipe
                dup2(stdout_write_side.as_raw_fd(), libc::STDOUT_FILENO).unwrap();
                dup2(stderr_write_side.as_raw_fd(), libc::STDERR_FILENO).unwrap();

                // TODO: Ignore (or pass through?) stdin

                //Exec the command
                self.exec().await.unwrap(); // Never returns - replaces currently runninng process
                                            // The panic just stops the compiler complaing that this branch does not
                                            // return
                unreachable!();
            }
        }
    }
}
