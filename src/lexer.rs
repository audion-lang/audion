// Copyright (C) 2025-2026 Aleksandr Bogdanov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//

use crate::error::{AudionError, Result};
use crate::token::{Span, Token, TokenKind};

/// All reserved keywords in the language.
pub const KEYWORDS: &[&str] = &[
    "fn", "let", "if", "else", "while", "loop", "for", "return",
    "thread", "define", "break", "continue", "this",
    "include", "as", "using", "true", "false", "nil",
];

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.is_at_end() {
                tokens.push(self.make_token(TokenKind::Eof));
                break;
            }
            let token = self.next_token()?;
            tokens.push(token);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token> {
        let start = self.pos;
        let ch = self.advance();

        let kind = match ch {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            ':' => {
                if self.match_char(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '.' => TokenKind::Dot,
            '+' => {
                if self.match_char('=') {
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.match_char('=') {
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.match_char('=') {
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.match_char('=') {
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => TokenKind::Percent,
            '=' => {
                if self.match_char('=') {
                    TokenKind::EqEq
                } else if self.match_char('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.match_char('=') {
                    TokenKind::BangEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.match_char('=') {
                    TokenKind::LtEq
                } else if self.match_char('<') {
                    TokenKind::LtLt
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.match_char('=') {
                    TokenKind::GtEq
                } else if self.match_char('>') {
                    TokenKind::GtGt
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                if self.match_char('&') {
                    TokenKind::And
                } else {
                    TokenKind::Ampersand
                }
            }
            '|' => {
                if self.match_char('|') {
                    TokenKind::Or
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => TokenKind::Caret,
            '~' => TokenKind::Tilde,
            '"' => self.read_string()?,
            c if c.is_ascii_digit() => self.read_number(start),
            c if c.is_ascii_alphabetic() || c == '_' => self.read_identifier(start),
            c => {
                return Err(AudionError::LexError {
                    msg: format!("unexpected character '{}'", c),
                    line: self.line,
                });
            }
        };

        Ok(Token {
            kind,
            span: Span {
                start,
                end: self.pos,
                line: self.line,
            },
        })
    }

    fn read_string(&mut self) -> Result<TokenKind> {
        let mut s = String::new();
        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                self.line += 1;
            }
            if self.peek() == '\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(AudionError::LexError {
                        msg: "unterminated string escape".to_string(),
                        line: self.line,
                    });
                }
                match self.advance() {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    '\\' => s.push('\\'),
                    '"' => s.push('"'),
                    c => {
                        s.push('\\');
                        s.push(c);
                    }
                }
            } else {
                s.push(self.advance());
            }
        }
        if self.is_at_end() {
            return Err(AudionError::LexError {
                msg: "unterminated string".to_string(),
                line: self.line,
            });
        }
        self.advance(); // closing "
        Ok(TokenKind::StringLit(s))
    }

    fn read_number(&mut self, _start: usize) -> TokenKind {
        while !self.is_at_end() && self.peek().is_ascii_digit() {
            self.advance();
        }
        if !self.is_at_end() && self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.advance(); // consume '.'
            while !self.is_at_end() && self.peek().is_ascii_digit() {
                self.advance();
            }
        }
        let text: String = self.source[_start..self.pos].iter().collect();
        TokenKind::Number(text.parse::<f64>().unwrap())
    }

    fn read_identifier(&mut self, start: usize) -> TokenKind {
        while !self.is_at_end() && (self.peek().is_ascii_alphanumeric() || self.peek() == '_') {
            self.advance();
        }
        let text: String = self.source[start..self.pos].iter().collect();
        match text.as_str() {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "for" => TokenKind::For,
            "return" => TokenKind::Return,
            "thread" => TokenKind::Thread,
            "define" => TokenKind::Define,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "this" => TokenKind::This,
            "include" => TokenKind::Include,
            "as" => TokenKind::As,
            "using" => TokenKind::Using,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            _ => TokenKind::Ident(text),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            if self.is_at_end() {
                return;
            }
            match self.peek() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.line += 1;
                    self.advance();
                }
                '/' => {
                    if self.peek_next() == '/' {
                        // line comment
                        while !self.is_at_end() && self.peek() != '\n' {
                            self.advance();
                        }
                    } else if self.peek_next() == '*' {
                        // block comment
                        self.advance(); // /
                        self.advance(); // *
                        while !self.is_at_end() {
                            if self.peek() == '\n' {
                                self.line += 1;
                            }
                            if self.peek() == '*' && self.peek_next() == '/' {
                                self.advance(); // *
                                self.advance(); // /
                                break;
                            }
                            self.advance();
                        }
                    } else {
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        ch
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.source[self.pos]
        }
    }

    fn peek_next(&self) -> char {
        if self.pos + 1 >= self.source.len() {
            '\0'
        } else {
            self.source[self.pos + 1]
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.source[self.pos] != expected {
            false
        } else {
            self.pos += 1;
            true
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn make_token(&self, kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span {
                start: self.pos,
                end: self.pos,
                line: self.line,
            },
        }
    }
}

