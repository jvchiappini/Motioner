# MDF v0 specification

Status: frozen v0 subset (2026-10-31). The first implementation covers
numbers, strings, booleans, lists, variables, functions, conditionals,
`for` loops, arithmetic, comparisons, calls, attribute access, the 2D
scene API, keyframes and linear/eased interpolation. `while`, `use`,
overrides and LaTeX are deferred to v0.1 and must not block the first
render milestone.

Language: Motioner Document Format (MDF).
Runtime: deterministic, sandboxed, program-first.

## 1. Design goals

- Program-first: the MDF program is the canonical source of a
  composition. Running it produces the document.
- Python-like: indentation blocks, functions, conditionals, loops,
  expressions and collections.
- Deterministic: the same program and inputs produce the same document
  and frames.
- Sandboxed: no filesystem, network, process or ambient time.
- Expressive enough to surpass Manim for procedural motion graphics.
- Readable by humans, editable by the AI and renderable by the editor.

## 2. Execution model

A program is executed once when the project is loaded or refreshed. It
builds a scene by calling constructors and setting properties. Execution
is depth-first and statements run in source order.

- Top-level statements run in the program scope.
- Functions run in an isolated call scope.
- Constructors create nodes and attach them to the current composition.
- Property assignment and keyframe calls mutate the created node.
- Running the program again rebuilds the document from scratch.
- UI overrides are applied after execution.

No I/O, no threads, no async and no external processes.

## 3. Lexical structure

- Source is UTF-8 text.
- A program may start with a version comment: `# mdf: 0.1`.
- Indentation uses spaces; four spaces per level. Tabs are rejected.
- Comments start with `#` and run to end of line.
- Blank lines and comment-only lines are ignored for indentation.
- Identifiers use letters, digits and underscores, and cannot start with
  a digit.
- Keywords: `def`, `return`, `if`, `elif`, `else`, `for`, `in`, `while`,
  `break`, `continue`, `pass`, `True`, `False`, `None`, `and`, `or`,
  `not`, `use`.
- Literals: integers, floats, strings (single or double quoted), raw
  strings prefixed with `r`, booleans, `None`, lists, tuples and dicts.
- Strings support `\n`, `\t`, `\\`, `\"`, `\'` and `\uXXXX`.

## 4. Types

Dynamic types: number, string, boolean, `None`, list, tuple, dict,
handle (scene node), function and module.

Numbers are IEEE-754 doubles in the script layer; rendering quantizes
them through the document model. Integer and float syntax are accepted;
there is no separate integer division rule: `/` always produces a float,
`//` produces an integer.

## 5. Statements

- Assignment: `name = expression`, `name += expression`, and the same for
  `-=`, `*=`, `/=`.
- Expression statement: a call or property assignment.
- Property assignment: `handle.property = expression`.
- Indexed assignment: `list[index] = expression`, `dict[key] = expression`.
- `def name(params):` followed by an indented block.
- `return expression` or `return`.
- `if condition:` / `elif` / `else`.
- `for name in iterable:` over lists, ranges, strings and dicts (keys in
  stable insertion order).
- `while condition:`.
- `break`, `continue`, `pass`.
- `use("path")` returns a module handle (see section 10).

## 6. Expressions

Supported operators, from lowest to highest precedence:

- `or`
- `and`
- `not`
- comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- `+`, `-`
- `*`, `/`, `//`, `%`
- unary `-`, `+`
- calls, attribute access, indexing
- literals, names, parenthesized expressions

Built-ins in v0: `len`, `range`, `str`, `int`, `float`, `bool`, `min`,
`max`, `abs`, `round`, `sorted`, `reversed`, `enumerate`, `zip`, `seed`,
`rand`, `sin`, `cos`, `tan`, `radians`, `degrees`, `lerp`, `clamp`.

`seed(value)` initializes the deterministic generator; `rand()` returns
a deterministic float in `[0, 1)`. Using `rand()` without `seed` is an
error.

## 7. Constructors and scene API

Constructors create nodes. Every node exposes properties and methods.
The v0 surface:

| Call | Purpose |
|---|---|
| `comp(name, width=1920, height=1080, fps=60, duration=5.0, background="#000000")` | Create the composition |
| `group()` | Empty container |
| `rect(width, height, radius=0, fill="#ffffff", stroke=None, stroke_width=0)` | Rectangle |
| `circle(radius, fill="#ffffff", stroke=None, stroke_width=0)` | Circle |
| `path(points, closed=False, fill=None, stroke="#ffffff", stroke_width=2)` | Vector path |
| `text(content, font="Inter", size=48, color="#ffffff", align="left")` | Text |
| `latex(expression, size=48, color="#ffffff", display=False)` | LaTeX math |
| `image(source, width=None, height=None, fit="contain")` | Image |
| `video(source, in_point=0.0, out_point=None)` | Video clip |
| `audio(source, in_point=0.0, out_point=None, gain=1.0)` | Audio clip |
| `camera(pos=(0, 0, 8), target=(0, 0, 0), fov=45)` | 3D camera |
| `light(kind="directional", intensity=1.0, color="#ffffff")` | 3D light |
| `mesh(source)` | glTF model |
| `particles(count=1000, seed=0, lifetime=2.0)` | Particle emitter |
| `use(path)` | Load a project-relative MDF module |

Common node members:

- `parent`, `children`, `name`, `id`, `enabled`.
- `pos`, `scale`, `rotation`, `anchor`, `opacity`, `blend`.
- `clip_in`, `clip_out`, `time_remap`.
- Methods: `add(*nodes)`, `remove()`, `key(property, time, value, ease="linear")`,
  `ease(name)`, `mask(node)`, `effect(kind, **params)`.

3D nodes add `target`, `fov`, `intensity`, `material`, `cast_shadow` and
similar properties. NLE nodes add clip points and track assignment.

Assignments use plain values:

```
title = text("Motioner", size=140)
title.pos = (960, 540)
title.opacity = 0
title.key("opacity", 0.0, 0.0)
title.key("opacity", 0.6, 1.0, ease="quad_out")
```

Keyframe times are in seconds and are converted to frames with the
composition fps. Values are lists or tuples for vectors.

## 8. Control flow example

```
def grid(count, spacing, radius):
    for i in range(count):
        dot = circle(radius=radius, fill="#ff3366")
        dot.pos = (360 + i * spacing, 800)
        dot.key("scale", 0.0 + i * 0.08, 0.0)
        dot.key("scale", 0.8 + i * 0.08, 1.0, ease="back_out")

grid(6, 220, 24)
```

## 9. Provenance and overrides

Every created node gets a provenance path derived from its source
location, function call stack and loop iteration. Explicit identities
are allowed with `.id("name")` and must be unique.

- The editor shows the compiled document.
- A UI edit creates an override keyed by provenance or explicit ID.
- Overrides are stored in `overrides.motion` as `override(id, property, value)`
  statements executed after the program.
- An override that no longer matches a node is reported as an orphan.
- Baking an element rewrites it into the program as explicit code and
  removes the override.

Overrides never silently change generated code.

## 10. Modules

`use("path/to/file.motion")` executes a project-relative module once and
returns a module handle. Top-level names become attributes on the module.
Paths are sandboxed to the project root; absolute paths and parent
escapes are rejected. Modules cannot perform I/O.

This is a v0 proposal and is open for review. If modules are deferred,
programs are self-contained and shared helpers are duplicated, which
conflicts with the project's DRY standard.

## 11. Determinism

- No ambient time, no wall-clock reads and no external state.
- `rand()` requires `seed()`.
- Iteration over dicts uses insertion order.
- Sorting is stable.
- Floating-point math uses the platform IEEE-754 implementation; rendered
  output is quantized by the document model.
- Same program and settings must produce the same document and the same
  frames on the same machine.
- Cross-machine differences are documented with tolerance.

## 12. Limits and sandbox

Configurable defaults for v0:

- Maximum statements executed: 10,000,000.
- Maximum function call depth: 256.
- Maximum nodes created: 1,000,000.
- Maximum string length: 16 MiB.
- Maximum script wall time: 5 seconds.
- Maximum memory: 256 MiB.

Exceeding a limit raises a runtime error with source span and stops
execution without corrupting the previous document.

## 13. Errors

Errors carry a category, message, source span and call stack.

Categories: lexical, parse, name, type, value, limit, unsupported.

The editor highlights the span, and the AI receives the same diagnostic
so it can repair the program.

## 14. Grammar (EBNF, v0 draft)

```
program        = { statement } ;
statement      = simple_stmt NEWLINE | compound_stmt ;
simple_stmt    = assignment | expression | return_stmt | break_stmt
               | continue_stmt | pass_stmt ;
assignment     = target ( "=" | "+=" | "-=" | "*=" | "/=" ) expression ;
target         = NAME { "." NAME | "[" expression "]" } ;
expression     = or_expr ;
or_expr        = and_expr { "or" and_expr } ;
and_expr       = not_expr { "and" not_expr } ;
not_expr       = [ "not" ] comparison ;
comparison     = additive { ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) additive } ;
additive       = multiplicative { ( "+" | "-" ) multiplicative } ;
multiplicative = unary { ( "*" | "/" | "//" | "%" ) unary } ;
unary          = [ "-" | "+" ] postfix ;
postfix        = primary { "." NAME | "[" expression "]" | "(" arguments ")" } ;
primary        = NAME | NUMBER | STRING | "True" | "False" | "None"
               | "(" expression ")" | list | tuple | dict ;
compound_stmt  = if_stmt | for_stmt | while_stmt | def_stmt ;
if_stmt        = "if" expression ":" block { "elif" expression ":" block }
               [ "else" ":" block ] ;
for_stmt       = "for" NAME "in" expression ":" block ;
while_stmt     = "while" expression ":" block ;
def_stmt       = "def" NAME "(" [ params ] ")" ":" block ;
block          = NEWLINE INDENT { statement } DEDENT ;
return_stmt    = "return" [ expression ] ;
break_stmt     = "break" ;
continue_stmt  = "continue" ;
pass_stmt      = "pass" ;
arguments      = [ expression { "," expression } ] ;
params         = NAME { "," NAME } ;
list           = "[" [ expression { "," expression } ] "]" ;
tuple          = "(" expression "," [ expression { "," expression } ] ")" ;
dict           = "{" [ expression ":" expression { "," expression ":" expression } ] "}" ;
```

## 15. Project layout

MDF programs live in a project folder:

```
<name>.motioner/
  project.motion          manifest and entry program
  compositions/*.motion   one program per composition
  overrides.motion        UI overrides applied after execution
  assets/                 content-addressed media
  cache/                  decoded media and frame caches
  backups/                autosave and recovery
```

The exact manifest schema is specified with the project format.

## 16. Example program

```
# mdf: 0.1

comp(name="Intro", width=1920, height=1080, fps=60, duration=6.0,
     background="#0b0b12")

title = text("Motioner", font="Inter", size=160, color="#ffffff")
title.pos = (960, 470)
title.opacity = 0.0
title.key("opacity", 0.0, 0.0)
title.key("opacity", 0.8, 1.0, ease="quad_out")

formula = latex(r"E = mc^2", size=72, color="#9ad1ff")
formula.pos = (960, 700)
formula.key("scale", 0.8, 0.6)
formula.key("scale", 1.6, 1.0, ease="back_out")

for i in range(5):
    dot = circle(radius=18, fill="#ff3366")
    dot.pos = (480 + i * 240, 900)
    dot.key("scale", 0.2 + i * 0.1, 0.0)
    dot.key("scale", 1.2 + i * 0.1, 1.0, ease="back_out")
```

## 17. Deferred to v0.1

1. `use` modules and shared helper files.
2. Override syntax, orphan reporting and bake behavior.
3. `while` loops.
4. F-strings and advanced formatting helpers.
5. LaTeX math commands and error policy.
6. Final resource-limit defaults.
7. Formatter and comment-preservation rules.

The v0 subset is frozen; these items have specifications in this
document but are not required for the first MP4 milestone.
