/// [`Span`] and [`ColumnEncoding`] live in the shared `parser-rs` crate
/// (a sibling checkout, pulled in as a path dependency).
pub use parser_rs::{ColumnEncoding, Span};

/// A Zinc source file: a sequence of libraries (Zinc has no code outside
/// of them).
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub libraries: Vec<Library>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Library {
    pub name: String,
    pub requires: Vec<Requirement>,
    pub members: Vec<Member>,
    pub line: usize,
}

/// One entry of a `requires`/`needs`/`uses` list.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    pub name: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Access {
    #[default]
    Default,
    Private,
    Public,
}

/// Declaration modifiers, in any order before the declaration itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Modifiers {
    /// Set either directly (`private function f`) or by an enclosing
    /// `private { ... }` / `public { ... }` block.
    pub access: Access,
    pub is_static: bool,
    pub is_constant: bool,
    pub is_readonly: bool,
    pub is_delegate: bool,
    pub is_stub: bool,
    pub is_optional: bool,
}

/// A library-level declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Var(VarDecl),
    Function(FunctionDecl),
    Struct(StructDecl),
    Interface(TypeBody),
    Module(TypeBody),
    /// `type name extends base[size];` — a dynamic array type.
    TypeDef {
        modifiers: Modifiers,
        name: String,
        extends: String,
        size: Option<Expr>,
        line: usize,
    },
}

/// A variable declaration: a global, a struct field, or a local. Zinc's
/// `integer a, b = 1;` declares two variables and parses into two of these.
#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub modifiers: Modifiers,
    pub type_name: String,
    pub name: String,
    /// One entry per `[...]` suffix: `integer a[10][20]` has two; `None`
    /// is an unsized `[]`. Empty for a non-array.
    pub dims: Vec<Option<Expr>>,
    pub initializer: Option<Expr>,
    pub line: usize,
}

impl VarDecl {
    pub fn is_array(&self) -> bool {
        !self.dims.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub type_name: String,
    pub name: String,
}

/// A function or method signature. Operator methods are named after the
/// operator, e.g. `operator []`.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSig {
    pub modifiers: Modifiers,
    pub name: String,
    pub params: Vec<Param>,
    /// `None` means no `-> type` (returns nothing).
    pub returns: Option<String>,
    pub line: usize,
}

/// A function, method, or anonymous function. `body` is `None` only for
/// an interface method signature (`method foo();`).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub sig: FunctionSig,
    pub body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub modifiers: Modifiers,
    pub name: String,
    /// `struct Data[8190]`: the instance limit.
    pub size: Option<Expr>,
    pub extends: Option<String>,
    pub members: Vec<StructMember>,
    pub line: usize,
}

/// The body shared by `interface`s and `module`s: just named members.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBody {
    pub modifiers: Modifiers,
    pub name: String,
    pub members: Vec<StructMember>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StructMember {
    Field(VarDecl),
    Method(FunctionDecl),
    /// `module Name;` (or `optional module Name;`) inside a struct.
    Implement {
        name: String,
        optional: bool,
        line: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Local(VarDecl),
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
        line: usize,
    },
    /// An expression evaluated for its effect — always a call.
    Expr {
        expr: Expr,
        line: usize,
    },
    If {
        is_static: bool,
        cond: Expr,
        then_body: Vec<Stmt>,
        /// `else if` is an `else` whose body is a single nested `If`.
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    DoWhile {
        body: Vec<Stmt>,
        cond: Expr,
        line: usize,
    },
    /// `for (start start_op var end_op end) body`, e.g. `for (0 <= i < n)`
    /// counts up, `for (n > i >= 0)` counts down.
    For {
        start: Expr,
        start_op: BinOp,
        var: String,
        var_span: Span,
        end_op: BinOp,
        end: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    Break {
        line: usize,
    },
    Return {
        value: Option<Expr>,
        line: usize,
    },
    /// `debug stmt` / `debug { ... }`: only compiled in debug mode.
    Debug {
        body: Vec<Stmt>,
        line: usize,
    },
    Block {
        body: Vec<Stmt>,
        line: usize,
    },
}

impl Stmt {
    pub fn line(&self) -> usize {
        match self {
            Stmt::Local(d) => d.line,
            Stmt::Assign { line, .. }
            | Stmt::Expr { line, .. }
            | Stmt::If { line, .. }
            | Stmt::While { line, .. }
            | Stmt::DoWhile { line, .. }
            | Stmt::For { line, .. }
            | Stmt::Break { line }
            | Stmt::Return { line, .. }
            | Stmt::Debug { line, .. }
            | Stmt::Block { line, .. } => *line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Compares only `kind`, ignoring `span` — spans are source-location
/// bookkeeping, not part of an expression's identity.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    IntLiteral(i64),
    RealLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    Null,
    This,
    ThisType,
    Var(String),
    /// `object.name`
    Member(Box<Expr>, String),
    /// `array[index]`
    Index(Box<Expr>, Box<Expr>),
    /// `callee(args)` — `callee` is any expression (`Foo`, `this.bar`, ...).
    Call(Box<Expr>, Vec<Expr>),
    /// `function Foo` / `function Data.onTimer`: a code reference.
    FuncRef(String),
    /// `function(params) -> type { ... }`: an anonymous function.
    AnonFunction(Box<FunctionDecl>),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Gt,
    Lt,
    GtEq,
    LtEq,
    And,
    Or,
}

impl BinOp {
    pub fn is_relational(self) -> bool {
        matches!(self, BinOp::Gt | BinOp::Lt | BinOp::GtEq | BinOp::LtEq)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ----- generic traversal helpers (shared by lint, query, ...) -----------

impl Stmt {
    /// The statement's own (direct) expressions, in source order.
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            Stmt::Local(d) => d.exprs(),
            Stmt::Assign { target, value, .. } => vec![target, value],
            Stmt::Expr { expr, .. } => vec![expr],
            Stmt::If { cond, .. } | Stmt::While { cond, .. } | Stmt::DoWhile { cond, .. } => {
                vec![cond]
            }
            Stmt::For { start, end, .. } => vec![start, end],
            Stmt::Return { value, .. } => value.iter().collect(),
            Stmt::Break { .. } | Stmt::Debug { .. } | Stmt::Block { .. } => Vec::new(),
        }
    }

    /// The statement's nested statement lists (`if` has up to two).
    pub fn bodies(&self) -> Vec<&[Stmt]> {
        match self {
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                let mut v = vec![then_body.as_slice()];
                v.extend(else_body.as_deref());
                v
            }
            Stmt::While { body, .. }
            | Stmt::DoWhile { body, .. }
            | Stmt::For { body, .. }
            | Stmt::Debug { body, .. }
            | Stmt::Block { body, .. } => vec![body],
            Stmt::Local(_)
            | Stmt::Assign { .. }
            | Stmt::Expr { .. }
            | Stmt::Break { .. }
            | Stmt::Return { .. } => Vec::new(),
        }
    }
}

impl VarDecl {
    /// Array sizes, then the initializer.
    pub fn exprs(&self) -> Vec<&Expr> {
        self.dims
            .iter()
            .flatten()
            .chain(self.initializer.as_ref())
            .collect()
    }
}

impl Expr {
    /// Direct sub-expressions, in source order. An anonymous function's
    /// body is a separate scope and is *not* included — see
    /// [`ExprKind::AnonFunction`].
    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Member(obj, _) => vec![obj],
            ExprKind::Index(arr, idx) => vec![arr, idx],
            ExprKind::Call(callee, args) => std::iter::once(&**callee).chain(args).collect(),
            ExprKind::Unary(_, e) => vec![e],
            ExprKind::Binary(l, _, r) => vec![l, r],
            ExprKind::IntLiteral(_)
            | ExprKind::RealLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::ThisType
            | ExprKind::Var(_)
            | ExprKind::FuncRef(_)
            | ExprKind::AnonFunction(_) => Vec::new(),
        }
    }

    /// `a`, `this.b`, `Data.create`: a dotted path, if this is one.
    pub fn path(&self) -> Option<String> {
        match &self.kind {
            ExprKind::Var(name) => Some(name.clone()),
            ExprKind::This => Some("this".to_string()),
            ExprKind::ThisType => Some("thistype".to_string()),
            ExprKind::Member(obj, name) => Some(format!("{}.{name}", obj.path()?)),
            _ => None,
        }
    }
}
