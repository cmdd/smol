//! Tokenizers which use regular expressions for their functionality.

use super::*;
use regex::Regex;

/// An iterator which returns tokens which match a regular expression.
pub struct RegexTokenIter<'a> {
    input: &'a str,
    regex: Regex,
    offset: usize,
    index: usize,
}

impl<'a> RegexTokenIter<'a> {
    pub fn new(input: &'a str, pattern: &str) -> RegexTokenIter<'a> {
        // TODO: Gracefully error
        let r = Regex::new(pattern).unwrap();
        RegexTokenIter {
            input: input,
            regex: r,
            offset: 0,
            index: 0,
        }
    }
}

impl<'a> Iterator for RegexTokenIter<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let m = self.regex.find(&self.input[self.offset..])?;
        self.index += 1;
        self.offset += m.end();

        Some(Token {
            term: m.as_str().into(),
            offset: m.start(),
            index: self.index - 1,
        })
    }
}
