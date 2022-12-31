# ttydo

Run a process, forcing allocation of a tty.

## Usage

Take any command and preffix with a call to `ttydo`. This is similar to `ssh`'s `-t` flag.

```
$ ttydo echo 'running with a pty'
running with a pty
```
