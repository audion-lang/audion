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

use crate::ast::*;
use crate::error::{AudionError, Result};
use crate::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    // --- Statements ---

    fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek_kind() {
            TokenKind::Let => self.parse_let(),
            TokenKind::Fn => self.parse_fn_decl(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::For => self.parse_for(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Break)
            }
            TokenKind::Continue => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Continue)
            }
            TokenKind::Thread => self.parse_thread(),
            TokenKind::Define => self.parse_synthdef(),
            TokenKind::Include => self.parse_include(),
            TokenKind::Using => self.parse_using(),
            TokenKind::LBrace => self.parse_block(),
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt> {
        let line = self.peek().span.line;
        self.advance(); // consume 'let'
        let name = self.expect_ident()?;
        let init = if self.match_token(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Let { name, init, line })
    }

    fn parse_fn_decl(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'fn'
        let name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RParen)?;
        let body = Box::new(self.parse_block()?);
        Ok(Stmt::FnDecl { name, params, body })
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>> {
        let mut params = Vec::new();
        if self.peek_kind() != TokenKind::RParen {
            params.push(self.parse_param()?);
            while self.match_token(TokenKind::Comma) {
                params.push(self.parse_param()?);
            }
        }
        Ok(params)
    }

    fn parse_param(&mut self) -> Result<Param> {
        let name = self.expect_ident()?;
        let default = if self.match_token(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Param { name, default })
    }

    /// SynthDef params are plain identifiers with no defaults (they map to UGen parameters).
    fn parse_synthdef_param_list(&mut self) -> Result<Vec<String>> {
        let mut params = Vec::new();
        if self.peek_kind() != TokenKind::RParen {
            params.push(self.expect_ident()?);
            while self.match_token(TokenKind::Comma) {
                params.push(self.expect_ident()?);
            }
        }
        Ok(params)
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'if'
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then = Box::new(self.parse_block_or_stmt()?);
        let else_ = if self.match_token(TokenKind::Else) {
            if self.peek_kind() == TokenKind::If {
                Some(Box::new(self.parse_if()?))
            } else {
                Some(Box::new(self.parse_block_or_stmt()?))
            }
        } else {
            None
        };
        Ok(Stmt::If { cond, then, else_ })
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'while'
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = Box::new(self.parse_block_or_stmt()?);
        Ok(Stmt::While { cond, body })
    }

    fn parse_loop(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'loop'
        let body = Box::new(self.parse_block()?);
        Ok(Stmt::Loop { body })
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'for'
        self.expect(TokenKind::LParen)?;

        // init
        let init = if self.match_token(TokenKind::Semicolon) {
            None
        } else if self.peek_kind() == TokenKind::Let {
            let s = self.parse_let()?; // includes semicolon
            Some(Box::new(s))
        } else {
            let line = self.peek().span.line;
            let expr = self.parse_expr()?;
            self.expect(TokenKind::Semicolon)?;
            Some(Box::new(Stmt::ExprStmt(expr, line)))
        };

        // cond
        let cond = if self.peek_kind() == TokenKind::Semicolon {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::Semicolon)?;

        // incr
        let incr = if self.peek_kind() == TokenKind::RParen {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::RParen)?;

        let body = Box::new(self.parse_block_or_stmt()?);
        Ok(Stmt::For {
            init,
            cond,
            incr,
            body,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'return'
        let value = if self.peek_kind() == TokenKind::Semicolon {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Return(value))
    }

    fn parse_thread(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'thread'
        let name = self.expect_ident()?;
        let body = Box::new(self.parse_block()?);
        Ok(Stmt::Thread { name, body })
    }

    fn parse_synthdef(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'define'
        let name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let params = self.parse_synthdef_param_list()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;

        // Parse optional let bindings before the final expressions
        let mut lets: Vec<(String, Box<UGenExpr>)> = Vec::new();
        while self.peek_kind() == TokenKind::Let {
            self.advance(); // consume 'let'
            let var_name = self.expect_ident()?;
            self.expect(TokenKind::Eq)?;
            let value = self.parse_ugen_expr()?;
            self.expect(TokenKind::Semicolon)?;
            lets.push((var_name, Box::new(value)));
        }

        // Parse one or more result expressions (e.g., multiple out() calls)
        let mut results: Vec<Box<UGenExpr>> = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let expr = self.parse_ugen_expr()?;
            self.expect(TokenKind::Semicolon)?;
            results.push(Box::new(expr));
        }
        self.expect(TokenKind::RBrace)?;

        let body = if lets.is_empty() && results.len() == 1 {
            // Simple case: no lets, single expression - unwrap it
            *results.into_iter().next().unwrap()
        } else {
            UGenExpr::Block {
                lets,
                results,
            }
        };

        Ok(Stmt::SynthDef { name, params, body })
    }

    fn parse_include(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'include'
        let path = if let TokenKind::StringLit(s) = self.peek_kind() {
            self.advance();
            s
        } else {
            return Err(self.error("expected string path after 'include'"));
        };

        // Check for optional 'as' alias: include "path" as ident(::ident)*;
        let alias = if self.match_token(TokenKind::As) {
            let mut segments = vec![self.expect_ident()?];
            while self.match_token(TokenKind::ColonColon) {
                segments.push(self.expect_ident()?);
            }
            Some(segments)
        } else {
            None
        };

        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Include { path, alias })
    }

    fn parse_using(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'using'
        let mut path = vec![self.expect_ident()?];
        while self.match_token(TokenKind::ColonColon) {
            path.push(self.expect_ident()?);
        }
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Using { path })
    }

    fn parse_ugen_expr(&mut self) -> Result<UGenExpr> {
        let mut left = self.parse_ugen_mul()?;
        loop {
            match self.peek_kind() {
                TokenKind::Plus => {
                    self.advance();
                    let right = self.parse_ugen_mul()?;
                    left = UGenExpr::BinOp {
                        left: Box::new(left),
                        op: BinOp::Add,
                        right: Box::new(right),
                    };
                }
                TokenKind::Minus => {
                    self.advance();
                    let right = self.parse_ugen_mul()?;
                    left = UGenExpr::BinOp {
                        left: Box::new(left),
                        op: BinOp::Sub,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_ugen_mul(&mut self) -> Result<UGenExpr> {
        let mut left = self.parse_ugen_postfix()?;
        loop {
            match self.peek_kind() {
                TokenKind::Star => {
                    self.advance();
                    let right = self.parse_ugen_postfix()?;
                    left = UGenExpr::BinOp {
                        left: Box::new(left),
                        op: BinOp::Mul,
                        right: Box::new(right),
                    };
                }
                TokenKind::Slash => {
                    self.advance();
                    let right = self.parse_ugen_postfix()?;
                    left = UGenExpr::BinOp {
                        left: Box::new(left),
                        op: BinOp::Div,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_ugen_postfix(&mut self) -> Result<UGenExpr> {
        let mut expr = self.parse_ugen_primary()?;
        loop {
            if self.match_token(TokenKind::LBracket) {
                let index = self.parse_ugen_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = UGenExpr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_ugen_primary(&mut self) -> Result<UGenExpr> {
        match self.peek_kind() {
            TokenKind::Number(n) => {
                self.advance();
                Ok(UGenExpr::Number(n))
            }
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(UGenExpr::StringLit(s))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_ugen_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.peek_kind() == TokenKind::LParen {
                    // UGen call
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    let mut named_args = Vec::new();
                    if self.peek_kind() != TokenKind::RParen {
                        self.parse_ugen_arg(&mut args, &mut named_args)?;
                        while self.match_token(TokenKind::Comma) {
                            self.parse_ugen_arg(&mut args, &mut named_args)?;
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(UGenExpr::UGenCall { name, args, named_args })
                } else {
                    // Parameter reference
                    Ok(UGenExpr::Param(name))
                }
            }
            _ => Err(self.error(&format!(
                "unexpected token {:?} in define body",
                self.peek_kind()
            ))),
        }
    }

    fn parse_ugen_arg(
        &mut self,
        args: &mut Vec<UGenExpr>,
        named_args: &mut Vec<(String, UGenExpr)>,
    ) -> Result<()> {
        // Check for named arg: ident ':' expr
        // Keywords like `loop` are valid named arg names in UGen context
        if let Some(name) = self.peek_as_ident_name() {
            if self.peek_kind_at(1) == TokenKind::Colon {
                self.advance(); // consume the identifier/keyword
                self.advance(); // consume ':'
                let value = self.parse_ugen_expr()?;
                named_args.push((name, value));
                return Ok(());
            }
        }
        args.push(self.parse_ugen_expr()?);
        Ok(())
    }

    /// Return the string name if the current token is an identifier or a keyword
    /// that can be used as a named argument in UGen context (e.g. `loop`).
    fn peek_as_ident_name(&self) -> Option<String> {
        match self.peek_kind() {
            TokenKind::Ident(name) => Some(name),
            TokenKind::Loop => Some("loop".to_string()),
            _ => None,
        }
    }

    fn parse_block(&mut self) -> Result<Stmt> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek_kind() != TokenKind::RBrace && !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_block_or_stmt(&mut self) -> Result<Stmt> {
        if self.peek_kind() == TokenKind::LBrace {
            self.parse_block()
        } else {
            self.parse_stmt()
        }
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt> {
        let line = self.peek().span.line;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::ExprStmt(expr, line))
    }

    // --- Expressions (Pratt parsing) ---

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let expr = self.parse_or()?;

        if self.match_token(TokenKind::Eq) {
            if let Expr::Ident(name) = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::Assign {
                    name,
                    value: Box::new(value),
                });
            }
            if let Expr::Index { object, index } = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::IndexAssign {
                    object,
                    index,
                    value: Box::new(value),
                });
            }
            if let Expr::MemberAccess { object, field } = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::MemberAssign {
                    object,
                    field,
                    value: Box::new(value),
                });
            }
            return Err(self.error("invalid assignment target"));
        }

        // Compound assignment: +=, -=, *=, /=, %=
        let op = match self.peek_kind() {
            TokenKind::PlusEq => Some(BinOp::Add),
            TokenKind::MinusEq => Some(BinOp::Sub),
            TokenKind::StarEq => Some(BinOp::Mul),
            TokenKind::SlashEq => Some(BinOp::Div),
            TokenKind::PercentEq => Some(BinOp::Mod),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            if let Expr::Ident(name) = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::CompoundAssign {
                    name,
                    op,
                    value: Box::new(value),
                });
            }
            if let Expr::Index { object, index } = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::CompoundIndexAssign {
                    object,
                    index,
                    op,
                    value: Box::new(value),
                });
            }
            if let Expr::MemberAccess { object, field } = expr {
                let value = self.parse_assignment()?;
                return Ok(Expr::CompoundMemberAssign {
                    object,
                    field,
                    op,
                    value: Box::new(value),
                });
            }
            return Err(self.error("invalid compound assignment target"));
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.match_token(TokenKind::Or) {
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_bit_or()?;
        while self.match_token(TokenKind::And) {
            let right = self.parse_bit_or()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_bit_xor()?;
        while self.match_token(TokenKind::Pipe) {
            let right = self.parse_bit_xor()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::BitOr,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr> {
        let mut left = self.parse_bit_and()?;
        while self.match_token(TokenKind::Caret) {
            let right = self.parse_bit_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::BitXor,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality()?;
        while self.match_token(TokenKind::Ampersand) {
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::BitAnd,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_bit_shift()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::LtEq,
                TokenKind::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_bit_shift()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_shift(&mut self) -> Result<Expr> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::LtLt => BinOp::LeftShift,
                TokenKind::GtGt => BinOp::RightShift,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr> {
        let base = self.parse_unary()?;
        if self.match_token(TokenKind::StarStar) {
            let exp = self.parse_power()?; // right-associative
            Ok(Expr::BinOp {
                left: Box::new(base),
                op: BinOp::Pow,
                right: Box::new(exp),
            })
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek_kind() {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::BitNot,
                    expr: Box::new(expr),
                })
            }
            TokenKind::PlusPlus => {
                self.advance();
                let expr = self.parse_call()?;
                if let Expr::Ident(name) = expr {
                    Ok(Expr::CompoundAssign {
                        name,
                        op: BinOp::Add,
                        value: Box::new(Expr::Number(1.0)),
                    })
                } else {
                    Err(self.error("++ requires a variable"))
                }
            }
            TokenKind::MinusMinus => {
                self.advance();
                let expr = self.parse_call()?;
                if let Expr::Ident(name) = expr {
                    Ok(Expr::CompoundAssign {
                        name,
                        op: BinOp::Sub,
                        value: Box::new(Expr::Number(1.0)),
                    })
                } else {
                    Err(self.error("-- requires a variable"))
                }
            }
            _ => self.parse_call(),
        }
    }

    fn parse_call(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_token(TokenKind::PlusPlus) {
                if let Expr::Ident(name) = expr {
                    expr = Expr::CompoundAssign {
                        name,
                        op: BinOp::Add,
                        value: Box::new(Expr::Number(1.0)),
                    };
                } else {
                    return Err(self.error("++ requires a variable"));
                }
            } else if self.match_token(TokenKind::MinusMinus) {
                if let Expr::Ident(name) = expr {
                    expr = Expr::CompoundAssign {
                        name,
                        op: BinOp::Sub,
                        value: Box::new(Expr::Number(1.0)),
                    };
                } else {
                    return Err(self.error("-- requires a variable"));
                }
            } else if self.match_token(TokenKind::LParen) {
                let args = self.parse_arg_list()?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else if self.match_token(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.match_token(TokenKind::Dot) {
                let field = self.expect_ident()?;
                expr = Expr::MemberAccess {
                    object: Box::new(expr),
                    field,
                };
            } else if self.match_token(TokenKind::ColonColon) {
                let name = self.expect_ident()?;
                expr = Expr::NamespaceAccess {
                    namespace: Box::new(expr),
                    name,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Arg>> {
        let mut args = Vec::new();
        if self.peek_kind() == TokenKind::RParen {
            return Ok(args);
        }

        args.push(self.parse_arg()?);
        while self.match_token(TokenKind::Comma) {
            args.push(self.parse_arg()?);
        }
        Ok(args)
    }

    fn parse_arg(&mut self) -> Result<Arg> {
        // Check for named arg: ident ':'  expr
        if let TokenKind::Ident(_) = self.peek_kind() {
            if self.peek_kind_at(1) == TokenKind::Colon {
                let name = self.expect_ident()?;
                self.advance(); // consume ':'
                let value = self.parse_expr()?;
                return Ok(Arg::Named { name, value });
            }
        }
        Ok(Arg::Positional(self.parse_expr()?))
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.peek().clone();
        match &token.kind {
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Number(n))
            }
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::StringLit(s))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::Nil => {
                self.advance();
                Ok(Expr::Nil)
            }
            TokenKind::This => {
                self.advance();
                Ok(Expr::This)
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expr::Ident(name))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Fn => {
                self.advance(); // consume 'fn'
                self.expect(TokenKind::LParen)?;
                let params = self.parse_param_list()?;
                self.expect(TokenKind::RParen)?;
                let body = Box::new(self.parse_block()?);
                Ok(Expr::FnExpr { params, body })
            }
            TokenKind::LBracket => {
                self.advance(); // consume '['
                let mut elements = Vec::new();
                if self.peek_kind() != TokenKind::RBracket {
                    elements.push(self.parse_array_element()?);
                    while self.match_token(TokenKind::Comma) {
                        // Allow trailing comma
                        if self.peek_kind() == TokenKind::RBracket {
                            break;
                        }
                        elements.push(self.parse_array_element()?);
                    }
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Expr::ArrayLit { elements })
            }
            _ => Err(self.error(&format!(
                "unexpected token {:?}",
                token.kind
            ))),
        }
    }

    fn parse_array_element(&mut self) -> Result<(Option<Expr>, Expr)> {
        let expr = self.parse_expr()?;
        if self.match_token(TokenKind::FatArrow) {
            // key => value
            let value = self.parse_expr()?;
            Ok((Some(expr), value))
        } else {
            // auto-indexed value
            Ok((None, expr))
        }
    }

    // --- Helpers ---

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens[self.pos].kind.clone()
    }

    fn peek_kind_at(&self, offset: usize) -> TokenKind {
        if self.pos + offset < self.tokens.len() {
            self.tokens[self.pos + offset].kind.clone()
        } else {
            TokenKind::Eof
        }
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.pos];
        if !self.is_at_end() {
            self.pos += 1;
        }
        token
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if std::mem::discriminant(&self.peek_kind()) == std::mem::discriminant(&kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<&Token> {
        if std::mem::discriminant(&self.peek_kind()) == std::mem::discriminant(&kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!(
                "expected {:?}, got {:?}",
                kind,
                self.peek_kind()
            )))
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        if let TokenKind::Ident(name) = self.peek_kind() {
            self.advance();
            Ok(name)
        } else {
            Err(self.error(&format!(
                "expected identifier, got {:?}",
                self.peek_kind()
            )))
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error(&self, msg: &str) -> AudionError {
        AudionError::ParseError {
            msg: msg.to_string(),
            line: self.peek().span.line,
        }
    }
}

