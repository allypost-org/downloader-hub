use std::process::{ExitStatus, Output};

use thiserror::Error;

use super::FixerError;

pub struct CmdOutput {
    inner: Output,
}
impl CmdOutput {
    pub fn is_success(&self) -> bool {
        self.inner.status.success()
    }

    pub fn status(&self) -> i32 {
        self.inner.status.code().unwrap_or(-1)
    }

    pub fn stdout_raw(&self) -> &[u8] {
        &self.inner.stdout
    }

    pub fn stdout(&self) -> Result<String, CmdOutputErr> {
        Self::decode_string(self.inner.stdout.clone())
    }

    pub fn stderr_raw(&self) -> &[u8] {
        &self.inner.stderr
    }

    pub fn stderr(&self) -> Result<String, CmdOutputErr> {
        Self::decode_string(self.inner.stderr.clone())
    }

    fn decode_string(bytes: Vec<u8>) -> Result<String, CmdOutputErr> {
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

impl From<Output> for CmdOutput {
    fn from(output: Output) -> Self {
        Self { inner: output }
    }
}

impl std::fmt::Debug for CmdOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[derive(Debug, Error)]
pub enum CmdError {
    #[error(transparent)]
    Run(std::io::Error),
    #[error(
        "Command {0} failed with status {status:?} and output {output:?}",
        status = .1.status(),
        output = .1.stderr(),
    )]
    Failed(String, CmdOutput),
    #[error("Command {0} failed with status {1:?}")]
    FailedStatus(String, ExitStatus),
    #[error(transparent)]
    Decode(#[from] CmdOutputErr),
    #[error("Command timed out")]
    Timeout,
}

#[derive(Debug, Error)]
pub enum CmdOutputErr {
    #[error(transparent)]
    DecodeError(#[from] std::string::FromUtf8Error),
}

impl From<CmdOutputErr> for FixerError {
    fn from(err: CmdOutputErr) -> Self {
        Self::CommandError(err.into())
    }
}
