# opencode prompt 14 — Motioner: the in-app agent harness (agent panel, real Anthropic provider, tools, vision, questions, budgets, traces)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`). Run this prompt **after**
`opencode-prompt-13-motion-std-and-agent-tools.md` **and**
`opencode-prompt-15-guimaker-design-system.md` (the agent panel is built on
GUIMaker, never on the old hand-placed rect API).

**Contracts — read them first:** `D-0012` (providers, keys in the Windows
Credential Manager, budgets, human approval), `D-0041` (harness, workflow,
clarifying questions, vision; **amended: no simulated providers**),
`D-0043` (tool catalogue), `D-0036` (write-through, traces), `D-0042`
(consent for sharing), `D-0024` (command layer).

Goal: a person opens Motioner, opens the **Agent panel**, pastes their
Anthropic API key once, and chats: "hazme un intro de 8 segundos para mi
canal de cocina". The agent asks what it really needs (with options and, for
style, rendered thumbnails), shows a short plan, builds the video through the
tools, looks at its own renders, fixes problems, and reports. Every change is
MDF code, undoable, reviewable per turn, and recorded as a trace.

**No mock, fake or simulated provider anywhere** — not in the product, not
in tests. The first provider is the real Anthropic Messages API; the user will
test it with their own key. Tests that need the network run only when a key
is present and are ignored otherwise; everything else (request building, SSE
parsing of real captured payloads, tool execution, context building, budgets,
UI state) is unit-tested offline.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
If the job must be split across runs, stop cleanly at a phase boundary and
report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the repository. Never edit `engine\ferrous`. Do not
  commit, stage, stash, reset, checkout, rebase or push. The reviewer
  commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Stop if free space on C: drops
  below 5 GB.
- `cargo fmt` (never `--all`), `cargo machete crates`. Clippy pedantic
  `-D warnings`; every `#[allow]` has a reason. `unsafe_code` stays denied.
- **Dependencies need an ADR first** (`pctl decision-new`), one ADR covering
  the agent stack: a blocking HTTPS client with rustls and streaming body
  reads (proposal: `ureq` with rustls; the app is not async — run requests on
  a worker thread with a channel to the UI), `serde`/`serde_json` (already in
  the lock file through the engine), and `keyring` for the Windows Credential
  Manager (safe API, no `unsafe` in Motioner). Justify each: licence, size,
  transitive deps, why writing it is worse.
- **New crate** `motioner_agent` (conversation model, providers, harness
  loop, context builder, budgets, sessions). Engine-free; network only here.
  `motioner_ui` owns the panel; `motioner_tools` (prompt 13) executes tools.
  Update `scripts/check_boundaries.py`.
- **Security**: the API key never touches the project, logs, traces, crash
  reports or the screen after entry (mask it). No shell, no arbitrary file
  access: the agent reaches files only through tools. Network only to the
  configured provider endpoint.
- Shell gotchas: Bash heredocs containing `'''` break (write files with the
  editor tool); PowerShell 5.1 drops unescaped `"` in native arguments;
  `core.autocrlf=true`.
- Screenshots of the panel: `scripts/ui_shot.ps1 -Out
  out/ui/p14-NN-<what>.png`.

Phase gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict.

---

## Phase 1 — Conversation model and sessions

- Provider-neutral content blocks: `Text`, `Thinking` (opaque: keep every
  provider field verbatim, including signatures, and send it back unchanged),
  `ToolUse { id, name, input }`, `ToolResult { tool_use_id, content: [Text |
  Image], is_error }`, `Image { media_type, bytes }`, `SystemNote` (a
  mid-conversation operator message).
- The history is **append-only**: no edit or deletion of earlier turns ever
  (required by preserved thinking on current Claude models; editing history
  invalidates thinking blocks). Project-state updates are appended as
  `SystemNote`s, never by rewriting the system prompt.
- Sessions persist under `<project>/agent/sessions/<id>.jsonl` (append per
  event) and resume after restart; images stored by hash under
  `<project>/agent/images/`. Each turn also emits a D-0036 trace record with
  conversation fields.

## Phase 2 — Anthropic provider (raw HTTP, no SDK exists for Rust)

Implement exactly this surface of the Messages API:

- `POST https://api.anthropic.com/v1/messages`, headers
  `content-type: application/json`, `x-api-key: <key>`,
  `anthropic-version: 2026-10-21`, and `anthropic-beta: <ids>` only when a
  beta feature is used.
- **Models**: default `claude-opus-5-5`; the settings list comes from
  `GET https://api.anthropic.com/v1/models` (show `display_name`, context
  `max_input_tokens`, output `max_tokens`), with `claude-sonnet-5-5`,
  `claude-haiku-5-5` and `claude-fable-5-1` as the known set when offline.
  Use model ids exactly as listed (never add date suffixes).
- **Streaming always** (`"stream": true`, `max_tokens` 64000). Parse SSE:
  `message_start`, `content_block_start`, `content_block_delta`
  (`text_delta`, `input_json_delta`, `thinking_delta`, `signature_delta`),
  `content_block_stop`, `message_delta` (`stop_reason`, `usage`),
  `message_stop`, `ping`, `error`. Unknown event or block types are kept and
  ignored, never fatal. Unit tests feed real captured streams (capture them
  once with the key during development and commit them as fixtures with the
  key-free payloads).
- **Thinking**: `"thinking": {"type": "adaptive", "display": "summarized"}`
  so the panel can show a readable reasoning summary. Never send
  `{"type": "disabled"}` or `budget_tokens` (both are rejected on current
  models). Depth is `"output_config": {"effort": "<low|medium|high|xhigh|max>"}`,
  set explicitly (default `high`; `claude-opus-5-5` would otherwise default to
  `medium`). Do not send `temperature`, `top_p` or `top_k` (rejected on API
  version 2026-10-21). No assistant prefill.
- **Tools**: definitions from the D-0043 catalogue with
  `"strict": true` (schemas with `additionalProperties: false` and
  `required`) and `"eager_input_streaming": true`. Accumulate
  `input_json_delta` fragments, parse strictly, **validate against the
  schema before running**; on failure return
  `{"type":"tool_result","tool_use_id":...,"is_error":true,"content":"{\"INVALID_JSON\": ...}"}`
  (built with the JSON library). `tool_choice` is always `{"type": "auto"}`
  (forced `any`/`tool` is rejected on current models); steer from the system
  prompt instead. Accept a tool name that differs only in letter case;
  otherwise return an `is_error` result that names the expected tool.
- **Parallel tool calls**: execute all `tool_use` blocks of a turn (in
  order, on the editor thread), then send **all** `tool_result` blocks in a
  single user message; failures are `is_error: true`, never dropped.
- **Images in tool results**: `content` is an array of
  `{"type":"text",...}` and
  `{"type":"image","source":{"type":"base64","media_type":"image/png","data":...}}`
  blocks (base64 without newlines). Respect the D-0043 image budget (default
  768 px long edge; the newest models accept up to 2576 px, keep ≤ 1568).
- **Stop reasons**: `end_turn` → turn done; `tool_use` → run tools and
  continue; `max_tokens` → never run a truncated tool call, continue the turn
  or report; `pause_turn` → resend to continue; `refusal` → read
  `stop_details` (category, explanation), show it to the user, never execute
  that turn's tools.
- **Refusal fallback** on by default: `"fallbacks": "default"` with
  `anthropic-beta: server-side-fallback-2026-07-01` on `claude-opus-5-5`,
  `claude-fable-5-1` and `claude-sonnet-5-5`; a setting turns it off. Show
  which model served a turn (`model` in the response).
- **Prompt caching**: tools and system prompt are byte-stable (deterministic
  tool order, no timestamps or per-request ids in them); send top-level
  `"cache_control": {"type": "ephemeral"}`; verify and display
  `usage.cache_read_input_tokens` / `cache_creation_input_tokens`. Volatile
  context goes in appended `{"role": "system", ...}` messages (they must
  follow a user message), which keep the cached prefix.
- **Errors**: retry 429 (honour `retry-after`), 529 and 5xx and connection
  errors with capped exponential backoff (max 3); never retry 400, 401, 403,
  404, 413 — show a clear, actionable message (invalid key, model not
  available, request too large). Cancellation aborts the stream immediately.
- **Token budgeting**: `POST /v1/messages/count_tokens` before a turn when
  the context is large; warn at 80% of the model's input limit.
- **Cost**: a price table in a config file (per model: input, output, cache
  read, cache write per million tokens; current values: Opus 5.5 $4/$20,
  cache read $0.20; Sonnet 5.5 $2/$10, cache read $0.20; Haiku 5.5
  $0.10/$0.50; Fable 5.1 $10/$50, cache read $0.25), editable, dated; the
  panel shows cost per turn and per task.
- **Key**: entered in settings, stored with `keyring` in the Windows
  Credential Manager; `ANTHROPIC_API_KEY` from the environment is accepted as
  a development fallback. A "Test connection" button sends a minimal request.

## Phase 3 — Harness loop and system prompt

- `crates/motioner_agent/prompts/system.md`: the system prompt, in English,
  versioned, written with care. It teaches the D-0041 workflow (understand →
  gaps → ask or assume → plan → build → verify → report), when to ask
  (≤ 3 questions with options and a recommended default; visual questions
  for style via `render_variants` + `ask_user_visual`; otherwise state
  assumptions in one line), the tool strategy (exact text tools first:
  `check_design`, `describe_frame`, `motion_report`; images for taste and
  user media; `marks` overlay to refer to elements), MDF and `std` essentials
  with short examples, the design rules summary, and the safety rules
  (approval for export and destructive actions). Keep the long reference
  (full `std` docs) behind `std_reference` instead of the prompt.
- Context builder: project summary, selection, playhead, scenes, pending
  overrides, MDF/`std` versions; on later turns only what changed, as
  appended system notes.
- Loop: stream → tools → results → continue, until `end_turn`, a user tool,
  cancel or a budget limit. Budgets per task (D-0012 defaults: 10 tool
  iterations of build/verify, 10 minutes, cost cap configurable; image-call
  limit from D-0043); the remaining budget is told to the model in each
  system note.
- Transactions: a checkpoint before each agent turn; the turn's changes are
  grouped for review and undo as one unit.
- User tools executed by the harness: `ask_user`, `ask_user_visual`,
  `show_plan`, `request_approval`, `remember_preference` (pause the loop
  until the user answers in the panel; the answer is the tool result).
- `export` always requires `request_approval` in the same turn; enforced by
  the harness, not only by the prompt.

## Phase 4 — Agent panel

- Docked panel: message list with streaming text; a collapsible "Reasoning"
  section per turn with the summarized thinking; tool calls as compact chips
  (name, short args, status, duration) that expand to the result; inline
  images (renders, filmstrips, variant grids) with click-to-zoom.
- Question UI: option pickers with the recommended option first and marked,
  free-text "Other", multi-select when asked; visual picker for variant
  grids; plan card with Approve / Edit.
- Review: per turn a diff of code changes with Accept / Reject (reject
  restores the checkpoint) and "Undo turn".
- Composer: text input, attachments (drop files → `import_asset` only after
  the user sends; current frame; selection), Stop button, budget and cost
  bar, model/effort selector.
- Settings: provider, model, effort, fallback toggle, key entry and test,
  budgets, image size, trace recording and sharing consent (D-0042, off by
  default for sharing).
- Everything the panel does goes through `Command`s where it changes editor
  state (D-0024), with parity tests.

## Phase 5 — Real end-to-end runs

- Ignored-by-default tests (`MOTIONER_E2E=1` and a key) that run real
  sessions on `examples/demo.motioner` copies: a clear brief (no questions
  expected), an ambiguous brief (questions expected), an edit request on an
  existing scene, a request that needs vision (fix a problem visible only in
  the render). Assert on outcomes (program runs, `check_design` has no
  errors, the expected tools were used, budgets respected), not on exact text.
- Run them once with the key available in the environment if present; if no
  key is available, leave them ready and say so in the report. Save the
  resulting sessions and traces under `out/agent/` for the reviewer.
- A short manual test script in `docs/agent.md` for the user: set the key,
  open the demo project, three prompts to try, what to look at.

## Phase 6 — Other real providers

After Anthropic is complete and green: OpenRouter and Google (Gemini) as
real providers behind the same trait (tool calls, image input, streaming),
each with its own request/stream mapping and captured-fixture parser tests,
and keys in the Credential Manager. If the job must stop earlier, backlog
them with a task each — never add a placeholder or simulated provider.

## Phase 7 — Docs, measurements, report

1. Docs: `docs/agent.md` (setup, providers, models, effort, budgets,
   privacy, troubleshooting by error), `docs/tools.md` updates, root
   `README.md`.
2. Measurements: time to first token in the panel, tool round-trip latency,
   cache hit rate over a 10-turn session, cost of the E2E sessions (if run).
3. ADR addendum to D-0041 with what was decided during implementation.
4. Tasks with `pctl task-new` per phase, `pctl session-log`; backlog: the
   Motioner model provider (local and API, after D-0040 training), context
   compaction for very long sessions, provider routing.
5. Final report (in Spanish): what changed per phase, screenshots of the
   panel, how the user connects their Claude key and what to try first, E2E
   results (or why they were not run), ADRs and dependencies, gate results,
   free space on C:, and what is left.
