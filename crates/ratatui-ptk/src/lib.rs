//! Building blocks for prompt_toolkit-style input on top of ratatui.
//!
//! [`PromptState`] holds the text, history, completion and search state and handles keys.
//! [`Prompt`] and [`CompletionMenu`] draw it; they take styles but pick no theme of their own
//! beyond plain ANSI defaults. [`Completer`] and [`Lexer`] are what an application implements
//! to plug in completion and highlighting.

mod buffer;
mod completion;
mod history;
mod keys;
mod lexer;
mod menu;
mod prompt;
mod state;
#[cfg(test)]
mod tests;

pub use buffer::TextBuffer;
pub use completion::{Completer, Completion};
pub use history::History;
pub use keys::{Command, KeyBindings};
pub use lexer::Lexer;
pub use menu::CompletionMenu;
pub use prompt::Prompt;
pub use state::{Outcome, PromptState};
