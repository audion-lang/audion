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

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    LeftShift,
    RightShift,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone)]
pub enum Arg {
    Positional(Expr),
    Named { name: String, value: Expr },
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    StringLit(String),
    Bool(bool),
    Nil,
    Ident(String),
    Assign {
        name: String,
        value: Box<Expr>,
    },
    CompoundAssign {
        name: String,
        op: BinOp,
        value: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    FnExpr {
        params: Vec<Param>,
        body: Box<Stmt>,
    },
    ArrayLit {
        elements: Vec<(Option<Expr>, Expr)>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    IndexAssign {
        object: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    CompoundIndexAssign {
        object: Box<Expr>,
        index: Box<Expr>,
        op: BinOp,
        value: Box<Expr>,
    },
    This,
    MemberAccess {
        object: Box<Expr>,
        field: String,
    },
    MemberAssign {
        object: Box<Expr>,
        field: String,
        value: Box<Expr>,
    },
    CompoundMemberAssign {
        object: Box<Expr>,
        field: String,
        op: BinOp,
        value: Box<Expr>,
    },
    NamespaceAccess {
        namespace: Box<Expr>,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub enum UGenExpr {
    UGenCall {
        name: String,
        args: Vec<UGenExpr>,
        named_args: Vec<(String, UGenExpr)>,
    },
    Param(String),
    Number(f64),
    StringLit(String),
    BinOp {
        left: Box<UGenExpr>,
        op: BinOp,
        right: Box<UGenExpr>,
    },
    Index {
        object: Box<UGenExpr>,
        index: Box<UGenExpr>,
    },
    /// Local variable bindings inside a define block.
    /// `lets` are emitted as SC `var` declarations + assignments.
    /// `results` are the final expressions (e.g., multiple out() calls).
    Block {
        lets: Vec<(String, Box<UGenExpr>)>,
        results: Vec<Box<UGenExpr>>,
    },
}

#[derive(Debug, Clone)]
pub enum Stmt {
    ExprStmt(Expr),
    Let {
        name: String,
        init: Option<Expr>,
    },
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then: Box<Stmt>,
        else_: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    Loop {
        body: Box<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        incr: Option<Expr>,
        body: Box<Stmt>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    FnDecl {
        name: String,
        params: Vec<Param>,
        body: Box<Stmt>,
    },
    Thread {
        name: String,
        body: Box<Stmt>,
    },
    SynthDef {
        name: String,
        params: Vec<String>,
        body: UGenExpr,
    },
    Include {
        path: String,
        alias: Option<Vec<String>>,
    },
    Using {
        path: Vec<String>,
    },
}
