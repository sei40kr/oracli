/// One candidate offered by a [`Completer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Text that replaces `start..cursor`.
    pub text: String,
    /// Byte offset where the replaced text starts.
    pub start: usize,
    /// Short note shown next to the candidate.
    pub meta: Option<String>,
}

impl Completion {
    pub fn new(text: impl Into<String>, start: usize) -> Self {
        Self {
            text: text.into(),
            start,
            meta: None,
        }
    }

    pub fn with_meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = Some(meta.into());
        self
    }
}

pub trait Completer {
    /// Candidates for the text with the cursor at byte offset `cursor`.
    fn complete(&self, text: &str, cursor: usize) -> Vec<Completion>;
}
