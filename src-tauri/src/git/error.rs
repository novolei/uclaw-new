//! Structured error type for the git module.

use std::fmt;
use std::io;
use std::string::FromUtf8Error;

/// All failure modes surfaced by the git module.
#[derive(Debug)]
pub(crate) enum GitError {
    Io(io::Error),
    Utf8(FromUtf8Error),
    NonZeroExit {
        program: String,
        args: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
        stdout: String,
    },
    MissingBinary(&'static str),
    NotARepository,
    NoBranch,
    NoWorkspaceChanges,
    CommitMessageRequired,
    EmptyCommitMessage,
    MissingRequired(&'static str),
    Parse(String),
    Internal(String),
}

impl GitError {
    #[must_use]
    pub fn from_output(program: &str, args: &[&str], output: &std::process::Output) -> Self {
        Self::NonZeroExit {
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        }
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "git io error: {error}"),
            Self::Utf8(error) => write!(f, "git output not valid UTF-8: {error}"),
            Self::NonZeroExit { program, args, exit_code: _, stderr, stdout } => {
                let detail = if stderr.is_empty() { stdout } else { stderr };
                if detail.is_empty() {
                    write!(f, "{program} {} failed", args.join(" "))
                } else {
                    write!(f, "{program} {} failed: {detail}", args.join(" "))
                }
            }
            Self::MissingBinary(name) => write!(f, "required binary not found on PATH: {name}"),
            Self::NotARepository => write!(f, "not a git repository"),
            Self::NoBranch => write!(f, "no branch is currently checked out (detached HEAD)"),
            Self::NoWorkspaceChanges => write!(f, "no workspace changes to commit"),
            Self::CommitMessageRequired => write!(f, "commit message is required"),
            Self::EmptyCommitMessage => write!(f, "commit message is empty"),
            Self::MissingRequired(field) => write!(f, "missing required input: {field}"),
            Self::Parse(detail) => write!(f, "git output parse error: {detail}"),
            Self::Internal(detail) => write!(f, "git internal error: {detail}"),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Utf8(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for GitError {
    fn from(error: io::Error) -> Self { Self::Io(error) }
}

impl From<FromUtf8Error> for GitError {
    fn from(error: FromUtf8Error) -> Self { Self::Utf8(error) }
}

impl From<GitError> for String {
    fn from(error: GitError) -> Self { error.to_string() }
}

/// Result alias used throughout the git module.
pub(crate) type GitResult<T> = Result<T, GitError>;
