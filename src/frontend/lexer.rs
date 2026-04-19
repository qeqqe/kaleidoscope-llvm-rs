use core::panic;

#[derive(Debug, PartialEq)]
pub enum Token {
    Eof,
    Def,
    Extern,
    Identifier(String),
    Number(f64),
    Char(char),
}

pub struct Lexer<I>
where
    I: Iterator<Item = char>,
{
    input: I,
    last_char: Option<char>,
}

impl<I> Lexer<I>
where
    I: Iterator<Item = char>,
{
    pub fn new(mut input: I) -> Self {
        let last_char: Option<char> = input.next();
        Self { input, last_char }
    }

    pub fn gettok(&mut self) -> Token {
        while self.last_char.is_some_and(|c| c.is_whitespace()) {
            self.last_char = self.input.next();
        }

        // first check for numeric value
        // (identifier methods or variables can't start with
        // numeric, so if start is numeric it's likely a number)
        // Number: [0-9.]+
        if self.last_char.is_some_and(|c| c.is_numeric()) {
            let mut num_val = String::new();
            while let Some(c) = self.last_char
                && (c.is_numeric() || c == '.')
            {
                num_val.push(c);
                self.last_char = self.input.next();
            }

            if !num_val.is_empty() && num_val.chars().filter(|&ch| ch == '.').count() <= 1 {
                return Token::Number(num_val.parse::<f64>().expect("Incorrect num_val parsing"));
            } else {
                panic!("Incorrect num_val {}", num_val);
            }
        }

        // identifier: [a-zA-Z][a-zA-Z0-9]*
        if self.last_char.is_some_and(|c| c.is_ascii_alphabetic()) {
            let mut identifier_str = String::new();

            while let Some(c) = self.last_char
                && c.is_alphanumeric()
            {
                identifier_str.push(c);
                self.last_char = self.input.next();
            }

            return match identifier_str.as_str() {
                "def" => Token::Def,
                "extern" => Token::Extern,

                _ => Token::Identifier(identifier_str),
            };
        }

        // returns Token::Eof if we reach the end of the file while skipping comments
        if self.last_char.is_some_and(|c| c == '#') {
            while self.last_char.is_some_and(|c| c != '\n' && c != '\r') {
                self.last_char = self.input.next();
            }

            if self.last_char.is_none() {
                return Token::Eof;
            }
        }
        match self.last_char {
            None => Token::Eof,
            Some(c) => {
                self.last_char = self.input.next();
                Token::Char(c)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_keyword() {
        let mut lex = Lexer::new("def extern def".chars());
        assert_eq!(Token::Def, lex.gettok());
        assert_eq!(Token::Extern, lex.gettok());
        assert_eq!(Token::Def, lex.gettok());
        assert_eq!(Token::Eof, lex.gettok());
    }

    #[test]
    fn test_numeric() {
        let mut lex = Lexer::new("1.234  123.21312 5644.44564                 13123.212".chars());
        assert_eq!(Token::Number(1.234), lex.gettok());
        assert_eq!(Token::Number(123.21312), lex.gettok());
        assert_eq!(Token::Number(5644.44564), lex.gettok());
        assert_eq!(Token::Number(13123.212), lex.gettok());
    }

    #[test]
    #[should_panic]
    fn test_invalid_numerics() {
        let mut lex = Lexer::new("127.0.0.1".chars());
        lex.gettok();
    }

    #[test]
    fn test_comments_and_chars() {
        let mut lex = Lexer::new("funny_function69420() # does funny things haha".chars());

        assert_eq!(Token::Identifier("funny".to_string()), lex.gettok());
        assert_eq!(Token::Char('_'), lex.gettok());
        assert_eq!(Token::Identifier("function69420".to_string()), lex.gettok());
        assert_eq!(Token::Char('('), lex.gettok());
        assert_eq!(Token::Char(')'), lex.gettok());
        assert_eq!(Token::Eof, lex.gettok());
    }
}
