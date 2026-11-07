# Write-through: the editor writes code

Every editing command (D-0024) that changes the document becomes a **code
edit** on the program text. The overrides file is only the fallback (D-0036).
One gesture is one program write and one undo entry, and undo restores the
exact previous bytes.

The corpus under `crates/motioner_dsl/tests/write_through/` contains one case
per strategy and per refusal; the examples below are taken from it.

## The strategy ladder

For each resolved change the planner tries, in order, and takes the first exact
result:

### 1. Private literal

The value comes from a literal evaluated once for this node.

Before:

```
title = text("Hi", color="#ffffff")
```

Command: `edit("…", "color", "#ff0000")`

After:

```
title = text("Hi", color="#ff0000")
```

### 2. Loop specialisation

The value is shared by every iteration. The planner inserts a guarded
statement inside the loop body, after the node's last write.

Before:

```
for i in range(3):
    c = circle(10)
    c.pos = (100 + i * 20, 200)
```

Command: `edit("…", "fill", "#ff0000")` on the first instance.

After:

```
for i in range(3):
    c = circle(10)
    c.pos = (100 + i * 20, 200)
    if i == 0:
        c.fill = "#ff0000"
```

Nested loops join with `and` (`if row == 1 and col == 1:`). Two iterations
that end with the same value merge into `if i in (1, 3):`; when every iteration
ends equal, the shared literal is edited instead. Keys use the same guard.

### 3. Call-site specialisation

The node is built by a function. If the function already takes the value as a
parameter, the call passes it; if the function returns the node, the result is
bound and assigned after the call; otherwise the literal is promoted to a
keyword parameter with its old text as the default.

Before:

```
def badge(x):
    c = circle(10, fill="#ffffff")
    c.pos = (x, 100)

badge(100)
badge(200)
```

Command: `edit("…", "fill", "#ff0000")` on the first badge.

After:

```
def badge(x, fill="#ffffff"):
    c = circle(10, fill=fill)
    c.pos = (x, 100)

badge(100, fill="#ff0000")
badge(200)
```

The re-run proves that the second call is unchanged.

### 4. Local assignment

A computed value on a bound node becomes an assignment after the node's last
write in its block:

```
c = circle(10)
c.rotation = 15
```

### 5. Fallback

When no exact code edit exists, the change stays as an override entry with a
reason, and the UI offers to convert it later. The planner never guesses.

## Structural gestures

Adding, deleting, duplicating, renaming, reordering, parenting, grouping,
ungrouping, placing assets and the key commands are code too:

```
add_layer("rect", width=200, height=120, after="…")
delete_layer("…")
duplicate_layer("…")
reorder_layer("…", 2)
reparent_layer("…", parent="…")
group_layers(["…", "…"], name="pair")
ungroup_layer("…")
```

They have no override equivalent. In `--overrides` mode, or for a project whose
edit mode is `overrides`, a structural gesture is refused with a typed error
instead of half-applying. Group and ungroup are planned as one combined edit,
so they are still one write and one undo step.

## Fallback cases that remain

- a layer created in a function and deleted outside it (delete it where it is
  authored);
- a shared `add()` call that cannot be split;
- reordering when the creation order is not the z order, or when siblings are
  created in loops or by calls;
- duplicating a node whose statements are not contiguous in its block;
- `locked`, expanded rows and panel layout, which are editor state stored in
  `editor.motion`, never program code.

Each refusal carries a reason shown in the overrides list.

## Safety

- Every program write is atomic (temp file + rename).
- Every code edit is verified by re-running the program and comparing the
  resulting document with the expected one, by position (D-0027 exactness).
- Undo stores the source and the overrides, so it restores both exactly.
- The same program, document and command always produce the same edit.

## Editing modes

`editor.motion` stores `edit_mode="code"` (default) or `"overrides"` per
project. `motioner edit` writes code unless the project says otherwise;
`--overrides` forces the D-0022 path for one run and `--code` forces
write-through. The View menu toggles the project setting.

The live control endpoint returns the same report as `motioner edit`: the
strategies, the overrides kept with their reasons, the project-relative file
and a unified diff of the code change.
