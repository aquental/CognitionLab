//! Control flow and errors (plan §3.5).
//!
//! Python uses exceptions for two different things: normal control flow that carries messages
//! (`InterruptAgentFlow`, `exceptions.py`) and real failures. Here they are different types:
//! [`Interrupt`] for the first, [`AgentError`] for the second, and [`StepError`] for a step that
//! can end either way.

use std::fmt;
use std::path::PathBuf;

use crate::merge::MergeError;
use crate::message::{ActionError, InvalidCost, Message};
use crate::template::TemplateError;

/// The kinds of `InterruptAgentFlow` (`exceptions.py`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InterruptKind {
    /// The task was submitted.
    Submitted,
    /// A step or cost limit was reached.
    LimitsExceeded,
    /// The wall-time limit was reached (a `LimitsExceeded` in Python).
    TimeExceeded,
    /// A human interrupted (interactive agent only).
    UserInterruption,
    /// The model reply had the wrong number of actions.
    FormatError,
}

impl InterruptKind {
    const ALL: [InterruptKind; 5] = [
        InterruptKind::Submitted,
        InterruptKind::LimitsExceeded,
        InterruptKind::TimeExceeded,
        InterruptKind::UserInterruption,
        InterruptKind::FormatError,
    ];

    /// The Python class name.
    pub fn name(self) -> &'static str {
        match self {
            InterruptKind::Submitted => "Submitted",
            InterruptKind::LimitsExceeded => "LimitsExceeded",
            InterruptKind::TimeExceeded => "TimeExceeded",
            InterruptKind::UserInterruption => "UserInterruption",
            InterruptKind::FormatError => "FormatError",
        }
    }

    /// The kind with this Python class name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// True for `LimitsExceeded` and its subclass `TimeExceeded`.
    pub fn is_limit(self) -> bool {
        matches!(
            self,
            InterruptKind::LimitsExceeded | InterruptKind::TimeExceeded
        )
    }
}

/// An interrupt and the messages it carries, like `InterruptAgentFlow(*messages)`.
///
/// It always holds at least one message, so the format-error handler cannot hit Python's
/// `IndexError` on an empty `FormatError` (plan §1.6).
#[derive(Clone, Debug, PartialEq)]
pub struct Interrupt {
    kind: InterruptKind,
    // Boxed so `Result<_, StepError>` stays small (clippy::result_large_err).
    head: Box<Message>,
    tail: Vec<Message>,
}

impl Interrupt {
    /// An interrupt with one or more messages.
    pub fn new(kind: InterruptKind, head: Message, tail: Vec<Message>) -> Self {
        Self {
            kind,
            head: Box::new(head),
            tail,
        }
    }

    /// `Submitted` with the exit message of `local.py:50-56`.
    pub fn submitted(submission: String) -> Self {
        let message = Message::exit(
            InterruptKind::Submitted.name(),
            submission.clone(),
            submission,
        );
        Self::new(InterruptKind::Submitted, message, Vec::new())
    }

    /// `LimitsExceeded` with the exit message of `default.py:132-138`.
    pub fn limits_exceeded() -> Self {
        Self::limit(InterruptKind::LimitsExceeded)
    }

    /// `TimeExceeded` with the exit message of `default.py:140-146`.
    pub fn time_exceeded() -> Self {
        Self::limit(InterruptKind::TimeExceeded)
    }

    fn limit(kind: InterruptKind) -> Self {
        Self::new(
            kind,
            Message::exit(kind.name(), kind.name(), ""),
            Vec::new(),
        )
    }

    /// An interrupt from a list of messages; `None` if the list is empty.
    pub fn from_messages(kind: InterruptKind, messages: Vec<Message>) -> Option<Self> {
        let mut messages = messages.into_iter();
        let head = messages.next()?;
        Some(Self::new(kind, head, messages.collect()))
    }

    /// Its kind.
    pub fn kind(&self) -> InterruptKind {
        self.kind
    }

    /// The first message (the one `FormatError` handling reads the cost from).
    pub fn head(&self) -> &Message {
        &self.head
    }

    /// The first message, mutably (the model adds cost and response to it).
    pub fn head_mut(&mut self) -> &mut Message {
        &mut self.head
    }

    /// All messages, in order.
    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        std::iter::once(&*self.head).chain(&self.tail)
    }

    /// All messages, in order, by value.
    pub fn into_messages(self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(1 + self.tail.len());
        messages.push(*self.head);
        messages.extend(self.tail);
        messages
    }
}

impl fmt::Display for Interrupt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.head.content)
    }
}

/// Failures of the model layer.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The deterministic model has no outputs left (Python raises `IndexError`).
    #[error("the deterministic model has no outputs left")]
    OutputsExhausted,
    /// A global cost or call limit was crossed (`models/__init__.py:30-31`, a `RuntimeError`).
    #[error("Global cost/call limit exceeded: ${cost:.4} / {calls}")]
    GlobalLimitExceeded {
        /// Global cost after this call.
        cost: f64,
        /// Global calls after this call.
        calls: u64,
    },
    /// A scripted `raise` of a non-interrupt exception.
    #[error("{message}")]
    Injected {
        /// The Python exception name, such as `RuntimeError`.
        kind: String,
        /// Its message.
        message: String,
    },
}

impl ModelError {
    /// The stable exit status name of this error (D6).
    pub fn exit_status(&self) -> &str {
        match self {
            ModelError::OutputsExhausted => "OutputsExhausted",
            ModelError::GlobalLimitExceeded { .. } => "GlobalLimitExceeded",
            ModelError::Injected { kind, .. } => kind,
        }
    }
}

/// Real failures. They end the run with an exit message and are returned to the caller.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// The model failed.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// A template failed to compile or render.
    #[error(transparent)]
    Template(#[from] TemplateError),
    /// An invalid configuration value.
    #[error("invalid configuration: {0}")]
    Config(String),
    /// A config merge failed.
    #[error(transparent)]
    Merge(#[from] MergeError),
    /// An assistant message holds invalid actions.
    #[error(transparent)]
    Action(#[from] ActionError),
    /// A message's `extra.cost` is not a number.
    #[error(transparent)]
    Cost(#[from] InvalidCost),
    /// A file operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// JSON conversion failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A broken internal invariant (never expected on a normal path).
    #[error("internal error: {0}")]
    Internal(String),
}

impl AgentError {
    /// The stable `exit_status` written into the exit message (D6). The parity comparator maps
    /// these names to Python's exception class names.
    pub fn exit_status(&self) -> &str {
        match self {
            AgentError::Model(error) => error.exit_status(),
            AgentError::Template(_) => "TemplateError",
            AgentError::Config(_) => "ConfigError",
            AgentError::Merge(_) => "MergeError",
            AgentError::Action(_) => "ActionError",
            AgentError::Cost(_) => "InvalidCost",
            AgentError::Io { .. } => "IoError",
            AgentError::Json(_) => "JsonError",
            AgentError::Internal(_) => "InternalError",
        }
    }
}

/// How a step, a query or a command can end besides success.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// Normal control flow that carries messages.
    #[error("{0}")]
    Interrupt(Interrupt),
    /// A real failure.
    #[error(transparent)]
    Fatal(#[from] AgentError),
}

impl From<Interrupt> for StepError {
    fn from(interrupt: Interrupt) -> Self {
        StepError::Interrupt(interrupt)
    }
}

// `?` performs one `From` conversion, so the common inner errors convert directly.
impl From<ModelError> for StepError {
    fn from(error: ModelError) -> Self {
        StepError::Fatal(error.into())
    }
}

impl From<TemplateError> for StepError {
    fn from(error: TemplateError) -> Self {
        StepError::Fatal(error.into())
    }
}

impl From<InvalidCost> for StepError {
    fn from(error: InvalidCost) -> Self {
        StepError::Fatal(error.into())
    }
}

impl From<ActionError> for StepError {
    fn from(error: ActionError) -> Self {
        StepError::Fatal(error.into())
    }
}
