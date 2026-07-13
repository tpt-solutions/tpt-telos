//! Recursive-descent parser for tpt-telos, building the AST.

use crate::ast::*;
use crate::lexer::{Token, lex};

pub struct Parser {
    tokens: Vec<(Token, usize, usize)>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<(Token, usize, usize)>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].0
    }

    fn advance(&mut self) -> Token {
        let (tok, _, _) = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        let got = self.peek().clone();
        if got == expected {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected {}, found {}", expected, got))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.pos += 1;
                Ok(s)
            }
            other => Err(format!("expected identifier, found {}", other)),
        }
    }

    // ---- program ----

    pub fn parse_source(src: &str) -> Result<Vec<Module>, String> {
        let tokens = lex(src)?;
        let mut p = Parser::new(tokens);
        let mut modules = Vec::new();
        while *p.peek() != Token::Eof {
            modules.push(p.parse_module()?);
        }
        Ok(modules)
    }

    fn parse_module(&mut self) -> Result<Module, String> {
        let mut attributes = Vec::new();
        while *self.peek() == Token::At {
            attributes.push(self.parse_attribute()?);
        }
        self.expect(Token::KwModule)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;
        let mut items = Vec::new();
        while *self.peek() != Token::RBrace {
            items.push(self.parse_item()?);
        }
        self.expect(Token::RBrace)?;
        Ok(Module {
            attributes,
            name,
            items,
        })
    }

    fn parse_attribute(&mut self) -> Result<Attribute, String> {
        self.expect(Token::At)?;
        let name = self.expect_ident()?;
        let mut args = Vec::new();
        if *self.peek() == Token::LParen {
            self.advance();
            if *self.peek() != Token::RParen {
                loop {
                    let key = self.expect_ident()?;
                    if *self.peek() == Token::Assign {
                        self.advance();
                        let lit = match self.peek().clone() {
                            Token::Int(n) => {
                                self.advance();
                                Literal::Int(n)
                            }
                            Token::Ident(s) => {
                                self.advance();
                                Literal::Ident(s)
                            }
                            other => return Err(format!("expected literal in attribute, found {}", other)),
                        };
                        args.push(Arg::Kv(key, lit));
                    } else {
                        args.push(Arg::Flag(key));
                    }
                    if *self.peek() == Token::Comma {
                        self.advance();
                        continue;
                    }
                    break;
                }
            }
            self.expect(Token::RParen)?;
        }
        Ok(Attribute { name, args })
    }

    fn parse_item(&mut self) -> Result<Item, String> {
        match self.peek() {
            Token::KwInvariant => Ok(Item::Invariant(self.parse_invariant()?)),
            Token::KwFunc => Ok(Item::Func(self.parse_func()?)),
            other => Err(format!("expected `invariant` or `func`, found {}", other)),
        }
    }

    fn parse_invariant(&mut self) -> Result<Invariant, String> {
        self.expect(Token::KwInvariant)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;
        let mut constraints = Vec::new();
        while *self.peek() != Token::RBrace {
            constraints.push(self.parse_expr()?);
            if *self.peek() == Token::Semicolon {
                self.advance();
            }
        }
        self.expect(Token::RBrace)?;
        Ok(Invariant { name, constraints })
    }

    fn parse_func(&mut self) -> Result<Func, String> {
        self.expect(Token::KwFunc)?;
        let name = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if *self.peek() != Token::RParen {
            loop {
                let pname = self.expect_ident()?;
                self.expect(Token::Colon)?;
                let ty = match self.peek().clone() {
                    Token::Ident(s) => {
                        self.advance();
                        Type::Named(s)
                    }
                    other => return Err(format!("expected type, found {}", other)),
                };
                params.push(Param { name: pname, ty });
                if *self.peek() == Token::Comma {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect(Token::RParen)?;

        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        while *self.peek() == Token::KwRequires || *self.peek() == Token::KwEnsures {
            if *self.peek() == Token::KwRequires {
                self.advance();
                requires.push(self.parse_expr()?);
            } else {
                self.advance();
                ensures.push(self.parse_expr()?);
            }
        }

        self.expect(Token::LBrace)?;
        let mut body = Vec::new();
        while *self.peek() != Token::RBrace {
            body.push(self.parse_stmt()?);
        }
        self.expect(Token::RBrace)?;

        Ok(Func {
            name,
            params,
            requires,
            ensures,
            body,
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if *self.peek() == Token::KwMutate {
            self.advance();
            self.expect(Token::KwState)?;
            self.expect(Token::LBrace)?;
            let mut assigns = Vec::new();
            while *self.peek() != Token::RBrace {
                assigns.push(self.parse_assign()?);
            }
            self.expect(Token::RBrace)?;
            return Ok(Stmt::MutateState(assigns));
        }
        Ok(Stmt::Assign(self.parse_assign()?))
    }

    fn parse_assign(&mut self) -> Result<Assign, String> {
        let target = self.parse_expr()?;
        let op = match self.peek().clone() {
            Token::Assign => AssignOp::Set,
            Token::PlusAssign => AssignOp::Add,
            Token::MinusAssign => AssignOp::Sub,
            other => return Err(format!("expected assignment operator, found {}", other)),
        };
        self.advance();
        let value = self.parse_expr()?;
        if *self.peek() == Token::Semicolon {
            self.advance();
        }
        Ok(Assign { target, op, value })
    }

    // ---- expressions ----

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_and()?;
        while *self.peek() == Token::Or {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Expr::Bin {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_cmp()?;
        while *self.peek() == Token::And {
            self.advance();
            let rhs = self.parse_cmp()?;
            lhs = Expr::Bin {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Token::EqEq => BinOp::Eq,
            Token::Ne => BinOp::Ne,
            Token::Lt => BinOp::Lt,
            Token::Le => BinOp::Le,
            Token::Gt => BinOp::Gt,
            Token::Ge => BinOp::Ge,
            _ => return Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_add()?;
        Ok(Expr::Bin {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Expr::Bin {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::Bin {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if *self.peek() == Token::Minus {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnOp::Neg,
                expr: Box::new(expr),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::KwOld => {
                self.advance();
                self.expect(Token::LParen)?;
                let e = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(Expr::Old(Box::new(e)))
            }
            Token::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(e)
            }
            Token::Ident(s) => {
                self.advance();
                if *self.peek() == Token::Dot {
                    self.advance();
                    let field = self.expect_ident()?;
                    Ok(Expr::Field { base: s, field })
                } else {
                    Ok(Expr::Var(s))
                }
            }
            other => Err(format!("expected expression, found {}", other)),
        }
    }
}

/// Convenience: parse a full source string into a list of modules.
pub fn parse(src: &str) -> Result<Vec<Module>, String> {
    Parser::parse_source(src)
}
