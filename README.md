# zinc-rs

A parser and linter for **Zinc**, the C-like Warcraft III scripting
dialect of vJASS. It's built the same way as [`jass-rs`](../jass-rs), on
top of the shared [`parser-rs`](../parser-rs) crate: spans, the lexer
cursor, literal scanners, the token stream, diagnostics, tree renderers and
the HTML AST viewer.

- [The Zinc language](#the-zinc-language) — a syntax reference to build on
- [Parser status](#parser-status) — what `zinc-rs` handles today
- [CLI](#cli) · [Tests](#tests)

How the parser is built, and the rules every parser here follows, is in
the [`parser-rs` README](../parser-rs/README.md#principles).

---

## The Zinc language

Zinc is an alternative syntax for vJASS, created by Vexorian and compiled
by JassHelper. It keeps vJASS's semantics (libraries, structs, modules,
...) but swaps JASS's keyword-heavy, line-based form for C-like syntax.
Statements end in `;`, blocks use braces, and `set`/`call`/`local` are
gone. Like vJASS, it all compiles down to plain [JASS](../jass-rs), so
JASS's type system, natives and runtime rules (array limits, no implicit
`real → integer`, ...) all still apply.

Zinc code lives in a vJASS map script between markers, or in a separate
file pulled in with `//! import zinc "file.zn"`:

```
//! zinc
library Example { ... }
//! endzinc
```

> This reference describes the syntax as `zinc-rs` models it. Check edge
> cases against the JassHelper manual (the "Zinc" section) before relying
> on them.

### Lexical structure

| element      | syntax                                                                       |
|--------------|------------------------------------------------------------------------------|
| statements   | end with `;`; line breaks are insignificant                                  |
| blocks       | `{ ... }`                                                                    |
| comments     | `// line` and `/* block */` (multi-line)                                     |
| identifiers  | as in JASS: letters, digits, `_`                                             |
| literals     | as in JASS: `42`, `0x1F`, `1.5`, `'hfoo'`, `"str\n"`, `true`, `false`, `null` |

**Keywords:** `library requires needs uses optional private public static
constant readonly delegate stub function method operator struct interface
module extends type if else while do for break return debug this thistype
true false null`.

**Operators:** `&& || !` (logical; replace `and or not`), `== != < > <= >=`,
`+ - * / %`, `=`, `+= -= *= /=`, `->` (return type), `.` (member access).

### Libraries

Everything lives inside a library. There is no code at file level.

```zinc
library Damage requires Utils, optional TimerUtils {   // also: needs / uses
    ...members...
}
```

- `requires` makes sure the listed libraries are placed before this one in
  the output. `optional` means "only if that library exists".
- **Library members are private by default**: they're visible only inside
  the library. Mark them `public` to export them. Modifiers can apply to a
  single declaration or to a whole group:

  ```zinc
  public function Api() { }
  private { integer counter; real ratio; }
  public  { function A() {} function B() {} }
  ```

- A library function named `onInit` runs automatically at map
  initialization.

### Variables

The syntax is the same for globals, struct fields and locals:

```zinc
integer x;                     // default value
integer a = 1, b, c = 3;       // several names, one type
constant real PI = 3.14159;    // must be initialized
unit hero = null;
integer arr[];                 // array (JASS-size)
integer sized[100];            // sized array
integer grid[10][10];          // 2-D array (emulated by the compiler)
thistype node;                 // inside a struct: the struct's own type
```

Locals are declared inside function bodies, with no `local` keyword.

### Functions

```zinc
function Add(integer a, integer b) -> integer {
    return a + b;
}

function Tick() {                              // no "->": returns nothing
    TimerStart(CreateTimer(), 1.0, true, function OnTick);   // code reference
    TimerStart(CreateTimer(), 0.5, false, function() {       // anonymous function
        DestroyTimer(GetExpiredTimer());
    });
}
```

Calls are plain `Foo(args);`, without `call`. Assignment is plain
`x = v;`, without `set`.

### Structs, interfaces, modules, types

```zinc
public struct Counter[8190] extends array {   // optional size, optional parent
    integer value;                             // instance field
    static integer created = 0;                // static field
    readonly integer id;                       // writable only inside the struct
    delegate Base base;                        // forwards unknown members
    module Pooled;                             // implement a module
    optional module Extra;                     // ...only if it exists

    static method create(integer v) -> thistype {
        thistype c = thistype.allocate();
        c.value = v;
        return c;
    }
    method onDestroy() { }
    stub method describe() -> string { return "counter"; }   // overridable

    method operator []  (integer i) -> integer { return 0; }
    method operator []= (integer i, integer v) { }
    method operator x=  (integer v) { this.value = v; }      // property setter
    method operator <   (thistype other) -> boolean { return this.value < other.value; }
}

interface Shape { method area() -> real; }                   // signatures only
module Pooled  { static method get() -> thistype { return 0; } }
type IntList extends integer[16];                            // dynamic array type
```

Struct members are public by default. Use `private` to hide one, either
directly or with a `private { }` group. `this` is the current instance and
`thistype` the current struct type.

### Statements

| statement   | syntax                                                                        |
|-------------|-------------------------------------------------------------------------------|
| assignment  | `x = e;` `a[i] = e;` `obj.f = e;` and `+= -= *= /=`                            |
| call        | `Foo(args);` `obj.method(args);` `Struct.staticMethod(args);`                  |
| if          | `if (cond) stmt-or-block [else stmt-or-block]`; `else if (...)` chains           |
| static if   | `static if (CONSTANT_BOOL) { }`: compile-time; the dead branch is dropped       |
| while       | `while (cond) stmt-or-block`                                                    |
| do-while    | `do { } while (cond);`                                                          |
| for         | `for (0 <= i < n) { }` counts up, `for (n > i >= 0) { }` counts down            |
| break       | `break;`: leaves the innermost loop                                             |
| return      | `return;` / `return expr;`                                                      |
| debug       | `debug stmt` / `debug { }`: only compiled in debug mode                         |
| block       | `{ ... }`                                                                       |

Conditions always take parentheses. A body can be a single statement or a
`{ block }`. In `for`, the counter variable must already be declared. Each
bound can use `<` / `<=` (counting up) or `>` / `>=` (counting down).

### Expressions

| precedence (low → high) | operators                               |
|-------------------------|-----------------------------------------|
| 1                       | `&&`                                    |
| 2                       | `\|\|`                                  |
| 3                       | `==` `!=`                               |
| 4                       | `<` `>` `<=` `>=`                       |
| 5                       | `+` `-`                                 |
| 6                       | `*` `/` `%`                             |
| 7                       | unary `-`, `!`                          |
| 8                       | postfix: `.member`, `[index]`, `(args)` |

Operands are literals, names, `this`, `thistype`, parenthesized
expressions, `function Name` / `function Struct.method` references and
anonymous `function(params) -> type { }`.

**Watch out:** the syntax looks like C, but the precedence is JASS's.
`&&` binds *looser* than `||`, so `false && true || true` means
`false && (true || true)`, which is `false`. Add parentheses when mixing
the two.

### Example

```zinc
//! zinc
library Counters requires optional TimerUtils {
    constant integer MAX = 8;
    integer counters[MAX];

    function Sum() -> integer {
        integer i, total = 0;
        for (0 <= i < MAX) {
            if (counters[i] > 0 && i != 3) { total += counters[i]; }
            else if (i > 6) { break; }
        }
        return total;
    }

    function onInit() {
        debug BJDebugMsg("sum: " + I2S(Sum()));
    }
}
//! endzinc
```

---

## Parser status

Supported: everything in the reference above. The parser:

- treats `//! zinc` / `//! endzinc` as ordinary comments, so a `.zn` file
  with or without the markers parses the same;
- is also the parser for Zinc **embedded in vJASS** `.j` files. `jass-rs`
  (`Dialect::VJass`) cuts each block out and calls `zinc_rs::parse_at(src,
  first_line)` (also `Lexer::tokenize_at`, `semantic_tokens_at`). All
  positions come back file-absolute;
- accepts modifiers syntactically but doesn't check where they're allowed
  (e.g. `readonly` on a library global);
- doesn't yet support `$1F` hex or `1.` / `.5` reals (same as `jass-rs`);
- does no symbol or type resolution: unknown names, wrong argument types
  and visibility violations aren't reported.

Lint rules: duplicate libraries, library members, struct members and
locals (including locals that shadow parameters), and `break` outside a
loop are errors. Unused locals, `while (true)` / `do … while (true)`
without `break`/`return`, empty `if`/`else` bodies, and a missing `return`
in a function that declares `-> type` are warnings. Anonymous functions
are checked as functions of their own.

## CLI

```sh
cargo run -- file.zn            # lint
cargo run -- --ast file.zn      # indented AST tree
cargo run -- --dot file.zn      # Graphviz DOT
cargo run -- --html file.zn > file.ast.html && open file.ast.html
```

## Tests

```sh
cargo test
UPDATE_SNAPSHOTS=1 cargo test --test html_snapshot   # regenerate HTML goldens
```
