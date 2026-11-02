use std::ops::Range;

pub trait Lexer {
    /// Byte ranges over the whole text, each tagged with a token class such as `"keyword"`.
    /// Ranges must not overlap; gaps are plain text. The class names are the application's;
    /// [`Prompt::syntax`](crate::Prompt::syntax) maps them to styles.
    fn lex(&self, text: &str) -> Vec<(Range<usize>, &'static str)>;
}
