use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Kw, Punct, Token, TokenKind};
use parser_rs::{TokenParser, TokenStream};

pub use parser_rs::ParseError;

type PResult<T> = Result<T, ParseError>;

pub struct Parser {
    ts: TokenStream<TokenKind>,
}

impl TokenParser for Parser {
    type Kind = TokenKind;

    fn tokens(&mut self) -> &mut TokenStream<TokenKind> {
        &mut self.ts
    }
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            ts: TokenStream::new(tokens),
        }
    }

    pub fn parse_str(src: &str) -> PResult<Program> {
        Parser::parse_str_at(src, 1)
    }

    /// Parses a region of a larger file starting on `first_line`. This is
    /// how a host parser (vJASS) hands a `//! zinc` block over: every span
    /// and error in the result is file-absolute.
    pub fn parse_str_at(src: &str, first_line: usize) -> PResult<Program> {
        Parser::new(Lexer::tokenize_at(src, first_line)?).program()
    }

    fn peek_kind(&self) -> &TokenKind {
        self.ts.peek_kind()
    }

    fn line(&self) -> usize {
        self.ts.peek().line
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.ts.check(kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        self.ts.eat(kind)
    }

    fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        self.ts.expect(kind)
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        self.ts.error(message)
    }

    fn ident(&mut self) -> PResult<String> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.ts.advance();
                Ok(name)
            }
            other => Err(self.error(format!("expected identifier, found {other}"))),
        }
    }

    fn type_name(&mut self) -> PResult<String> {
        if self.eat(&TokenKind::Keyword(Kw::ThisType)) {
            return Ok("thistype".to_string());
        }
        self.ident()
    }

    /// Whether the next tokens start a variable declaration (`type name`).
    fn at_var_decl(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Ident(_) | TokenKind::Keyword(Kw::ThisType)
        ) && matches!(self.ts.nth_kind(1), TokenKind::Ident(_))
    }

    // ----- declarations -------------------------------------------------

    /// Libraries, global-scope declarations and misplaced statements, in
    /// any order.
    pub fn program(&mut self) -> PResult<Program> {
        let mut program = Program::default();
        while !self.ts.at_eof() {
            if self.check(&TokenKind::Keyword(Kw::Library)) {
                program.libraries.push(self.library()?);
            } else if self.at_member() {
                self.member(Access::Default, Self::library_member, &mut program.globals)?;
            } else {
                self.stmt(&mut program.stmts)?;
            }
        }
        Ok(program)
    }

    /// Whether the next tokens start a library-level declaration, rather
    /// than a statement.
    fn at_member(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Keyword(Kw::Static) => self.ts.nth_kind(1) != &TokenKind::Keyword(Kw::If),
            TokenKind::Keyword(
                Kw::Private
                | Kw::Public
                | Kw::Constant
                | Kw::Readonly
                | Kw::Delegate
                | Kw::Stub
                | Kw::Optional
                | Kw::Function
                | Kw::Struct
                | Kw::Interface
                | Kw::Module
                | Kw::Type,
            ) => true,
            _ => self.at_var_decl(),
        }
    }

    fn library(&mut self) -> PResult<Library> {
        let line = self.line();
        if !self.check(&TokenKind::Keyword(Kw::Library)) {
            return Err(self.error(format!(
                "expected 'library', found {}",
                self.peek_kind()
            )));
        }
        self.ts.advance();
        let name = self.ident()?;
        let mut requires = Vec::new();
        if matches!(
            self.peek_kind(),
            TokenKind::Keyword(Kw::Requires) | TokenKind::Keyword(Kw::Needs) | TokenKind::Keyword(Kw::Uses)
        ) {
            self.ts.advance();
            loop {
                let optional = self.eat(&TokenKind::Keyword(Kw::Optional));
                requires.push(Requirement {
                    name: self.ident()?,
                    optional,
                });
                if !self.eat(&TokenKind::Punct(Punct::Comma)) {
                    break;
                }
            }
        }
        self.expect(TokenKind::Punct(Punct::LBrace))?;
        let mut members = Vec::new();
        self.members(Access::Default, Self::library_member, &mut members)?;
        self.expect(TokenKind::Punct(Punct::RBrace))?;
        Ok(Library {
            name,
            requires,
            members,
            line,
        })
    }

    /// Parses members up to (not including) the closing `}` into `out`,
    /// flattening `private { ... }` / `public { ... }` groups. Library and
    /// struct bodies share this loop (and the modifier handling); `item`
    /// parses one declaration after its modifiers.
    fn members<M>(
        &mut self,
        access: Access,
        item: fn(&mut Self, Modifiers, usize) -> PResult<Vec<M>>,
        out: &mut Vec<M>,
    ) -> PResult<()> {
        while !self.check(&TokenKind::Punct(Punct::RBrace)) {
            if self.ts.at_eof() {
                return Err(self.error("unexpected end of file, expected '}'"));
            }
            self.member(access, item, out)?;
        }
        Ok(())
    }

    /// One declaration, or one `private { ... }` / `public { ... }` group.
    fn member<M>(
        &mut self,
        access: Access,
        item: fn(&mut Self, Modifiers, usize) -> PResult<Vec<M>>,
        out: &mut Vec<M>,
    ) -> PResult<()> {
        let group_access = match (self.peek_kind(), self.ts.nth_kind(1)) {
            (TokenKind::Keyword(Kw::Private), TokenKind::Punct(Punct::LBrace)) => Some(Access::Private),
            (TokenKind::Keyword(Kw::Public), TokenKind::Punct(Punct::LBrace)) => Some(Access::Public),
            _ => None,
        };
        if let Some(group_access) = group_access {
            self.ts.advance();
            self.ts.advance();
            self.members(group_access, item, out)?;
            self.expect(TokenKind::Punct(Punct::RBrace))?;
            return Ok(());
        }
        let line = self.line();
        let modifiers = self.modifiers(access);
        out.extend(item(self, modifiers, line)?);
        Ok(())
    }

    fn modifiers(&mut self, access: Access) -> Modifiers {
        let mut m = Modifiers {
            access,
            ..Modifiers::default()
        };
        loop {
            match self.peek_kind() {
                TokenKind::Keyword(Kw::Private) => m.access = Access::Private,
                TokenKind::Keyword(Kw::Public) => m.access = Access::Public,
                TokenKind::Keyword(Kw::Static) => m.is_static = true,
                TokenKind::Keyword(Kw::Constant) => m.is_constant = true,
                TokenKind::Keyword(Kw::Readonly) => m.is_readonly = true,
                TokenKind::Keyword(Kw::Delegate) => m.is_delegate = true,
                TokenKind::Keyword(Kw::Stub) => m.is_stub = true,
                TokenKind::Keyword(Kw::Optional) => m.is_optional = true,
                _ => return m,
            }
            self.ts.advance();
        }
    }

    fn library_member(&mut self, modifiers: Modifiers, line: usize) -> PResult<Vec<Member>> {
        Ok(match self.peek_kind() {
            TokenKind::Keyword(Kw::Function) => {
                self.ts.advance();
                let name = self.ident()?;
                vec![Member::Function(self.function_rest(modifiers, name, line)?)]
            }
            TokenKind::Keyword(Kw::Struct) => vec![Member::Struct(self.struct_decl(modifiers, line)?)],
            TokenKind::Keyword(Kw::Interface) => {
                self.ts.advance();
                vec![Member::Interface(self.type_body(modifiers, line)?)]
            }
            TokenKind::Keyword(Kw::Module) => {
                self.ts.advance();
                vec![Member::Module(self.type_body(modifiers, line)?)]
            }
            TokenKind::Keyword(Kw::Type) => {
                self.ts.advance();
                let name = self.ident()?;
                self.expect(TokenKind::Keyword(Kw::Extends))?;
                let extends = self.type_name()?;
                let size = if self.eat(&TokenKind::Punct(Punct::LBracket)) {
                    let size = self.expr()?;
                    self.expect(TokenKind::Punct(Punct::RBracket))?;
                    Some(size)
                } else {
                    None
                };
                self.expect(TokenKind::Punct(Punct::Semicolon))?;
                vec![Member::TypeDef {
                    modifiers,
                    name,
                    extends,
                    size,
                    line,
                }]
            }
            _ if self.at_var_decl() => self
                .var_decls(modifiers, line)?
                .into_iter()
                .map(Member::Var)
                .collect(),
            other => {
                return Err(self.error(format!("unexpected token in library body: {other}")));
            }
        })
    }

    fn struct_member(&mut self, modifiers: Modifiers, line: usize) -> PResult<Vec<StructMember>> {
        Ok(match self.peek_kind() {
            TokenKind::Keyword(Kw::Method) => {
                self.ts.advance();
                let name = if self.eat(&TokenKind::Keyword(Kw::Operator)) {
                    self.operator_name()?
                } else {
                    self.ident()?
                };
                vec![StructMember::Method(
                    self.function_rest(modifiers, name, line)?,
                )]
            }
            TokenKind::Keyword(Kw::Module) => {
                self.ts.advance();
                let name = self.ident()?;
                self.expect(TokenKind::Punct(Punct::Semicolon))?;
                vec![StructMember::Implement {
                    name,
                    optional: modifiers.is_optional,
                    line,
                }]
            }
            _ if self.at_var_decl() => self
                .var_decls(modifiers, line)?
                .into_iter()
                .map(StructMember::Field)
                .collect(),
            other => {
                return Err(self.error(format!("unexpected token in struct body: {other}")));
            }
        })
    }

    /// The name of an operator method: `operator []`, `operator []=`,
    /// `operator <`, `operator x=` (a property setter), ...
    fn operator_name(&mut self) -> PResult<String> {
        let mut name = String::from("operator ");
        while !self.check(&TokenKind::Punct(Punct::LParen)) {
            let tok = self.ts.advance();
            match &tok.kind {
                TokenKind::Ident(s) => name.push_str(s),
                TokenKind::Punct(p) => name.push_str(p.text()),
                k => {
                    return Err(ParseError::at_token(
                        format!("unexpected token in operator name: {k}"),
                        &tok,
                    ));
                }
            }
        }
        if name == "operator " {
            return Err(self.error("expected operator name"));
        }
        Ok(name)
    }

    /// Everything after a function/method name: `(params) [-> type]`
    /// followed by a `{ body }`, or by `;` for a bodiless signature.
    fn function_rest(
        &mut self,
        modifiers: Modifiers,
        name: String,
        line: usize,
    ) -> PResult<FunctionDecl> {
        let (params, returns) = self.signature()?;
        let body = if self.eat(&TokenKind::Punct(Punct::Semicolon)) {
            None
        } else {
            Some(self.block()?)
        };
        Ok(FunctionDecl {
            sig: FunctionSig {
                modifiers,
                name,
                params,
                returns,
                line,
            },
            body,
        })
    }

    fn signature(&mut self) -> PResult<(Vec<Param>, Option<String>)> {
        self.expect(TokenKind::Punct(Punct::LParen))?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::Punct(Punct::RParen)) {
            loop {
                let type_name = self.type_name()?;
                let name = self.ident()?;
                params.push(Param { type_name, name });
                if !self.eat(&TokenKind::Punct(Punct::Comma)) {
                    break;
                }
            }
        }
        self.expect(TokenKind::Punct(Punct::RParen))?;
        let returns = if self.eat(&TokenKind::Punct(Punct::Arrow)) {
            Some(self.type_name()?)
        } else {
            None
        };
        Ok((params, returns))
    }

    fn struct_decl(&mut self, modifiers: Modifiers, line: usize) -> PResult<StructDecl> {
        self.expect(TokenKind::Keyword(Kw::Struct))?;
        let name = self.ident()?;
        let size = if self.eat(&TokenKind::Punct(Punct::LBracket)) {
            let size = self.expr()?;
            self.expect(TokenKind::Punct(Punct::RBracket))?;
            Some(size)
        } else {
            None
        };
        let extends = if self.eat(&TokenKind::Keyword(Kw::Extends)) {
            Some(self.type_name()?)
        } else {
            None
        };
        self.expect(TokenKind::Punct(Punct::LBrace))?;
        let mut members = Vec::new();
        self.members(Access::Default, Self::struct_member, &mut members)?;
        self.expect(TokenKind::Punct(Punct::RBrace))?;
        Ok(StructDecl {
            modifiers,
            name,
            size,
            extends,
            members,
            line,
        })
    }

    /// `Name { members }` of an interface or module (keyword already eaten).
    fn type_body(&mut self, modifiers: Modifiers, line: usize) -> PResult<TypeBody> {
        let name = self.ident()?;
        self.expect(TokenKind::Punct(Punct::LBrace))?;
        let mut members = Vec::new();
        self.members(Access::Default, Self::struct_member, &mut members)?;
        self.expect(TokenKind::Punct(Punct::RBrace))?;
        Ok(TypeBody {
            modifiers,
            name,
            members,
            line,
        })
    }

    /// `type a [dims] [= init], b ... ;` — one [`VarDecl`] per name.
    fn var_decls(&mut self, modifiers: Modifiers, line: usize) -> PResult<Vec<VarDecl>> {
        let type_name = self.type_name()?;
        let mut decls = Vec::new();
        loop {
            let decl_line = if decls.is_empty() { line } else { self.line() };
            let name = self.ident()?;
            let mut dims = Vec::new();
            while self.eat(&TokenKind::Punct(Punct::LBracket)) {
                if self.eat(&TokenKind::Punct(Punct::RBracket)) {
                    dims.push(None);
                } else {
                    dims.push(Some(self.expr()?));
                    self.expect(TokenKind::Punct(Punct::RBracket))?;
                }
            }
            let initializer = if self.eat(&TokenKind::Punct(Punct::Assign)) {
                Some(self.expr()?)
            } else {
                None
            };
            decls.push(VarDecl {
                modifiers: modifiers.clone(),
                type_name: type_name.clone(),
                name,
                dims,
                initializer,
                line: decl_line,
            });
            if !self.eat(&TokenKind::Punct(Punct::Comma)) {
                break;
            }
        }
        self.expect(TokenKind::Punct(Punct::Semicolon))?;
        Ok(decls)
    }

    // ----- statements ---------------------------------------------------

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(TokenKind::Punct(Punct::LBrace))?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Punct(Punct::RBrace)) {
            if self.ts.at_eof() {
                return Err(self.error("unexpected end of file, expected '}'"));
            }
            self.stmt(&mut stmts)?;
        }
        self.expect(TokenKind::Punct(Punct::RBrace))?;
        Ok(stmts)
    }

    /// A `{ block }` or a single statement (as the body of `if`/`while`/...).
    fn body(&mut self) -> PResult<Vec<Stmt>> {
        if self.check(&TokenKind::Punct(Punct::LBrace)) {
            self.block()
        } else {
            let mut stmts = Vec::new();
            self.stmt(&mut stmts)?;
            Ok(stmts)
        }
    }

    fn paren_expr(&mut self) -> PResult<Expr> {
        self.expect(TokenKind::Punct(Punct::LParen))?;
        let e = self.expr()?;
        self.expect(TokenKind::Punct(Punct::RParen))?;
        Ok(e)
    }

    /// Parses one statement into `out` — usually one [`Stmt`], but a
    /// multi-name declaration (`integer a, b;`) yields one per name.
    fn stmt(&mut self, out: &mut Vec<Stmt>) -> PResult<()> {
        let line = self.line();
        let stmt = match self.peek_kind() {
            TokenKind::Punct(Punct::LBrace) => Stmt::Block {
                body: self.block()?,
                line,
            },
            TokenKind::Keyword(Kw::If) => self.if_stmt(false)?,
            TokenKind::Keyword(Kw::Static) if self.ts.nth_kind(1) == &TokenKind::Keyword(Kw::If) => {
                self.ts.advance();
                self.if_stmt(true)?
            }
            TokenKind::Keyword(Kw::While) => {
                self.ts.advance();
                let cond = self.paren_expr()?;
                let body = self.body()?;
                Stmt::While { cond, body, line }
            }
            TokenKind::Keyword(Kw::Do) => {
                self.ts.advance();
                let body = self.block()?;
                self.expect(TokenKind::Keyword(Kw::While))?;
                let cond = self.paren_expr()?;
                self.expect(TokenKind::Punct(Punct::Semicolon))?;
                Stmt::DoWhile { body, cond, line }
            }
            TokenKind::Keyword(Kw::For) => self.for_stmt()?,
            TokenKind::Keyword(Kw::Break) => {
                self.ts.advance();
                self.expect(TokenKind::Punct(Punct::Semicolon))?;
                Stmt::Break { line }
            }
            TokenKind::Keyword(Kw::Return) => {
                self.ts.advance();
                let value = if self.check(&TokenKind::Punct(Punct::Semicolon)) {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(TokenKind::Punct(Punct::Semicolon))?;
                Stmt::Return { value, line }
            }
            TokenKind::Keyword(Kw::Debug) => {
                self.ts.advance();
                let body = self.body()?;
                Stmt::Debug { body, line }
            }
            TokenKind::Keyword(Kw::Constant) => {
                let modifiers = self.modifiers(Access::Default);
                out.extend(self.var_decls(modifiers, line)?.into_iter().map(Stmt::Local));
                return Ok(());
            }
            _ if self.at_var_decl() => {
                out.extend(
                    self.var_decls(Modifiers::default(), line)?
                        .into_iter()
                        .map(Stmt::Local),
                );
                return Ok(());
            }
            _ => self.simple_stmt(line)?,
        };
        out.push(stmt);
        Ok(())
    }

    fn if_stmt(&mut self, is_static: bool) -> PResult<Stmt> {
        let line = self.line();
        self.expect(TokenKind::Keyword(Kw::If))?;
        let cond = self.paren_expr()?;
        let then_body = self.body()?;
        let then_end = self.prev_span();
        let (else_body, else_span) = if self.check(&TokenKind::Keyword(Kw::Else)) {
            let start = self.ts.peek().span();
            self.eat(&TokenKind::Keyword(Kw::Else));
            (Some(self.body()?), Some(start.join(self.prev_span())))
        } else {
            (None, None)
        };
        Ok(Stmt::If {
            is_static,
            cond,
            then_body,
            else_body,
            line,
            else_span,
            then_end,
        })
    }

    /// `for (start op var op end) body`. The header is parsed as one
    /// ordinary expression — relational operators are left-associative, so
    /// `0 <= i < n` is `(0 <= i) < n` — and then taken apart.
    fn for_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(TokenKind::Keyword(Kw::For))?;
        let open = self.expect(TokenKind::Punct(Punct::LParen))?;
        let header = self.expr()?;
        self.expect(TokenKind::Punct(Punct::RParen))?;
        let malformed = || {
            ParseError::at_token(
                "expected a range loop header like 'for (0 <= i < n)'",
                &open,
            )
        };
        let ExprKind::Binary(lower, end_op, end) = header.kind else {
            return Err(malformed());
        };
        let ExprKind::Binary(start, start_op, var) = lower.kind else {
            return Err(malformed());
        };
        let ExprKind::Var(var_name) = var.kind else {
            return Err(malformed());
        };
        if !start_op.is_relational() || !end_op.is_relational() {
            return Err(malformed());
        }
        let body = self.body()?;
        Ok(Stmt::For {
            start: *start,
            start_op,
            var: var_name,
            var_span: var.span,
            end_op,
            end: *end,
            body,
            line,
        })
    }

    /// An assignment (`target op= value;`) or a call statement (`f();`).
    fn simple_stmt(&mut self, line: usize) -> PResult<Stmt> {
        let start = self.ts.peek().clone();
        let target = self.expr()?;
        let op = match self.peek_kind() {
            TokenKind::Punct(Punct::Assign) => Some(AssignOp::Assign),
            TokenKind::Punct(Punct::PlusAssign) => Some(AssignOp::Add),
            TokenKind::Punct(Punct::MinusAssign) => Some(AssignOp::Sub),
            TokenKind::Punct(Punct::StarAssign) => Some(AssignOp::Mul),
            TokenKind::Punct(Punct::SlashAssign) => Some(AssignOp::Div),
            _ => None,
        };
        let stmt = match op {
            Some(op) => {
                if !matches!(
                    target.kind,
                    ExprKind::Var(_) | ExprKind::Member(..) | ExprKind::Index(..)
                ) {
                    return Err(ParseError::at_token(
                        "invalid assignment target",
                        &start,
                    ));
                }
                self.ts.advance();
                let value = self.expr()?;
                Stmt::Assign {
                    target,
                    op,
                    value,
                    line,
                }
            }
            None if matches!(target.kind, ExprKind::Call(..)) => Stmt::Expr { expr: target, line },
            None => {
                return Err(ParseError::at_token(
                    "expected an assignment or a call",
                    &start,
                ));
            }
        };
        self.expect(TokenKind::Punct(Punct::Semicolon))?;
        Ok(stmt)
    }

    // ----- expressions --------------------------------------------------

    pub fn expr(&mut self) -> PResult<Expr> {
        self.and_expr()
    }

    /// Zinc keeps JASS's precedence (it compiles `&&`/`||` straight to
    /// `and`/`or`), so — unlike C — `&&` binds *looser* than `||`:
    /// `false && true || true` is `false && (true || true)`, i.e. `false`.
    fn and_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::or_expr,
            |k| matches!(k, TokenKind::Punct(Punct::AndAnd)).then_some(BinOp::And),
            binary,
        )
    }

    fn or_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::equality_expr,
            |k| matches!(k, TokenKind::Punct(Punct::OrOr)).then_some(BinOp::Or),
            binary,
        )
    }

    fn equality_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::relational_expr,
            |k| match k {
                TokenKind::Punct(Punct::EqEq) => Some(BinOp::Eq),
                TokenKind::Punct(Punct::NotEq) => Some(BinOp::NotEq),
                _ => None,
            },
            binary,
        )
    }

    fn relational_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::additive_expr,
            |k| match k {
                TokenKind::Punct(Punct::Gt) => Some(BinOp::Gt),
                TokenKind::Punct(Punct::Lt) => Some(BinOp::Lt),
                TokenKind::Punct(Punct::GtEq) => Some(BinOp::GtEq),
                TokenKind::Punct(Punct::LtEq) => Some(BinOp::LtEq),
                _ => None,
            },
            binary,
        )
    }

    fn additive_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::multiplicative_expr,
            |k| match k {
                TokenKind::Punct(Punct::Plus) => Some(BinOp::Add),
                TokenKind::Punct(Punct::Minus) => Some(BinOp::Sub),
                _ => None,
            },
            binary,
        )
    }

    fn multiplicative_expr(&mut self) -> PResult<Expr> {
        self.left_assoc(
            Self::unary_expr,
            |k| match k {
                TokenKind::Punct(Punct::Star) => Some(BinOp::Mul),
                TokenKind::Punct(Punct::Slash) => Some(BinOp::Div),
                TokenKind::Punct(Punct::Percent) => Some(BinOp::Mod),
                _ => None,
            },
            binary,
        )
    }

    fn unary_expr(&mut self) -> PResult<Expr> {
        let op = match self.peek_kind() {
            TokenKind::Punct(Punct::Minus) => UnaryOp::Neg,
            TokenKind::Punct(Punct::Bang) => UnaryOp::Not,
            _ => return self.postfix_expr(),
        };
        let op_tok = self.ts.advance();
        let e = self.unary_expr()?;
        let span = op_tok.span().join(e.span);
        Ok(Expr::new(ExprKind::Unary(op, Box::new(e)), span))
    }

    fn postfix_expr(&mut self) -> PResult<Expr> {
        let mut e = self.primary_expr()?;
        loop {
            match self.peek_kind() {
                TokenKind::Punct(Punct::Dot) => {
                    self.ts.advance();
                    let name_tok = self.ts.peek().clone();
                    let name = self.ident()?;
                    let span = e.span.join(name_tok.span());
                    e = Expr::new(ExprKind::Member(Box::new(e), name), span);
                }
                TokenKind::Punct(Punct::LBracket) => {
                    self.ts.advance();
                    let idx = self.expr()?;
                    let close = self.expect(TokenKind::Punct(Punct::RBracket))?;
                    let span = e.span.join(close.span());
                    e = Expr::new(ExprKind::Index(Box::new(e), Box::new(idx)), span);
                }
                TokenKind::Punct(Punct::LParen) => {
                    self.ts.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::Punct(Punct::RParen)) {
                        loop {
                            args.push(self.expr()?);
                            if !self.eat(&TokenKind::Punct(Punct::Comma)) {
                                break;
                            }
                        }
                    }
                    let close = self.expect(TokenKind::Punct(Punct::RParen))?;
                    let span = e.span.join(close.span());
                    e = Expr::new(ExprKind::Call(Box::new(e), args), span);
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary_expr(&mut self) -> PResult<Expr> {
        let tok = self.ts.peek().clone();
        let leaf = |p: &mut Self, kind| {
            p.ts.advance();
            Ok(Expr::new(kind, tok.span()))
        };
        match tok.kind.clone() {
            TokenKind::Int(v) => leaf(self, ExprKind::IntLiteral(v)),
            TokenKind::Real(v) => leaf(self, ExprKind::RealLiteral(v)),
            TokenKind::Str(v) => leaf(self, ExprKind::StringLiteral(v)),
            TokenKind::Keyword(Kw::True) => leaf(self, ExprKind::BoolLiteral(true)),
            TokenKind::Keyword(Kw::False) => leaf(self, ExprKind::BoolLiteral(false)),
            TokenKind::Keyword(Kw::Null) => leaf(self, ExprKind::Null),
            TokenKind::Keyword(Kw::This) => leaf(self, ExprKind::This),
            TokenKind::Keyword(Kw::ThisType) => leaf(self, ExprKind::ThisType),
            TokenKind::Ident(name) => leaf(self, ExprKind::Var(name)),
            TokenKind::Punct(Punct::LParen) => {
                self.ts.advance();
                let e = self.expr()?;
                let close = self.expect(TokenKind::Punct(Punct::RParen))?;
                Ok(Expr::new(e.kind, tok.span().join(close.span())))
            }
            TokenKind::Keyword(Kw::Function) => {
                self.ts.advance();
                if self.check(&TokenKind::Punct(Punct::LParen)) {
                    let (params, returns) = self.signature()?;
                    let body = self.block()?;
                    let end = self.prev_span();
                    let decl = FunctionDecl {
                        sig: FunctionSig {
                            modifiers: Modifiers::default(),
                            name: String::new(),
                            params,
                            returns,
                            line: tok.line,
                        },
                        body: Some(body),
                    };
                    return Ok(Expr::new(
                        ExprKind::AnonFunction(Box::new(decl)),
                        tok.span().join(end),
                    ));
                }
                let mut name = self.ident()?;
                let mut end = self.prev_span();
                while self.eat(&TokenKind::Punct(Punct::Dot)) {
                    name.push('.');
                    name.push_str(&self.ident()?);
                    end = self.prev_span();
                }
                Ok(Expr::new(ExprKind::FuncRef(name), tok.span().join(end)))
            }
            other => Err(self.error(format!("unexpected token in expression: {other}"))),
        }
    }

    /// The span of the token just consumed.
    fn prev_span(&self) -> Span {
        self.ts.prev().span()
    }
}

fn binary(left: Expr, op: BinOp, right: Expr) -> Expr {
    let span = left.span.join(right.span);
    Expr::new(ExprKind::Binary(Box::new(left), op, Box::new(right)), span)
}

#[cfg(test)]
#[path = "parser.test.rs"]
mod tests;
