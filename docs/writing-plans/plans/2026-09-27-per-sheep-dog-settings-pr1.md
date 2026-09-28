# Per-sheep dog settings, PR 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A sheep's Flockfile entry carries `[app.dogs.<dog>]`, the shepherd stores it as a Live `AppConfig` field, and a dog reads its tables, writes one, and hears when one changes.

**Architecture:** One new shep-core type (`DogTable`), one new `AppConfig` field, two requests, two responses and one bus event. The daemon's write reuses `SetSheepField`'s override path. The event fires from one reconciliation point after every mailbox message, so no registration or deletion path can forget it.

**Spec:** `docs/brainstorming/specs/2026-09-27-per-sheep-dog-settings-design.md`. Read it first. PR 2 (the lookout sub-screen) gets its own plan later.

**Tech stack:** Rust 2024, MSRV 1.88. serde_json, schemars 1.2, toml 0.8, insta. No new dependencies.

## Global constraints

- **Invoke `rust-house-style` before writing Rust**, and read `docs/rust-house-style-addendum.md`. Most common drift here: `std::error::Error` instead of `core::error::Error`, missing `# Errors`, a derived `Debug` on anything carrying a table.
- **One cargo shape for every task in this plan:**
  ```bash
  cargo test --workspace --lib --bins --all-features -- --skip ::slow::
  ```
  ```bash
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  ```
  Do not add `-p <crate>` runs "to catch failures early": switching shapes rebuilds half the tree. Run one cargo command at a time. Capture `$?` directly, never through a pipe.
- **Gate every commit, not the branch.** Both commands above pass before each `git commit`, and `cargo fmt --all --check` too.
- **Commit the moment it compiles.** Never `git checkout -- <file>` on a file with uncommitted work; undo a mutation with the opposite edit.
- **Conventional commit subjects**, `type(scope): summary`. No `!` anywhere in this plan: `MIN_SUPPORTED` does not move and no peer is refused. `revert` and `build` are refused by the hook.
- **File size (IR-48).** `.github/rust-file-size-baseline.txt` lists files that may shrink and never grow. Touching one of them in this plan is a bug: `supervisor/tests/mod.rs`, `boot_order.rs`, `testing.rs` (daemon), every listed lookout file. New behaviour goes in new files. `verbs.rs` leaves the list in Task 1.
- **Comments (IR-47).** Say only what the code cannot. No history, no dates, no em dashes. About one prose comment line per commit is the measured norm.
- **Never name the private dog that motivated this.** Examples use a dog called `jobs`.
- **When this plan is wrong about the code, follow the code** and say so in the task report. Every snippet about existing code here is a guess with a grep anchor.

## Interface contract

Fixed across tasks. Later tasks grep for these names.

```rust
// shep_core::config (new file config/app/dog_table.rs, re-exported)
// wire format: changing this is a breaking change
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DogTable(serde_json::Map<String, serde_json::Value>);
impl DogTable {
    pub fn as_map(&self) -> &serde_json::Map<String, serde_json::Value>;
    pub fn into_map(self) -> serde_json::Map<String, serde_json::Value>;
}
impl From<serde_json::Map<String, serde_json::Value>> for DogTable;
// Debug prints exactly `DogTable(<N keys>)`, `<1 key>` for one.
// Deserialize: an object only. Every nested object that is exactly
// {"$__toml_private_datetime": "<s>"} becomes the string "<s>", in objects
// and in arrays, at any depth.

// AppConfig
pub dogs: BTreeMap<String, DogTable>, // serde default, empty

// shep_core::protocol
Request::DogSheepSettings { dog: String }
Request::SetSheepDogSettings { name: String, dog: String, table: Option<DogTable> }
Response::DogSheepSettings { tables: BTreeMap<String, DogTable> } // sheep name -> table
Response::SheepDogSettingsSet { name: String, dog: String }
BusEvent::DogSheepSettingsChanged { dog: String, sheep: String } // topic config.sheep.<dog>

// shep_client::dogs
pub fn parse_sheep_settings<S: DeserializeOwned>(
    dog: &str, sheep: &str, table: &DogTable,
) -> Result<S, SheepSettingsError>;
// SheepSettingsError { dog, sheep }, Display:
// "the [app.dogs.<dog>] table on <sheep> does not fit this dog's settings"
```

## Dispatch buckets

Three implementer sessions, run one after another in this worktree (a shared cargo lock rules out two at once). Each ends with a review before the next starts.

| bucket | tasks | model | why |
| --- | --- | --- | --- |
| A | 1 to 4, shep-core | sonnet, high | one crate, known shapes |
| B | 5 to 7, shep-daemon | opus, medium | writes the operator's live override store |
| C | 8 and 9, client and docs | sonnet, high | small code, docs gate |

Estimated wall-clock: A about 75 minutes, B about 90, C about 50, plus reviews.

---

### Task 1: Shrink `verbs.rs` under the IR-48 line

`verbs.rs` is 1,004 lines and baselined. `request_wire_snapshots` builds 34 `Envelope { id, deadline_ms, body }` literals by hand.

**Files:** `crates/shep-core/src/protocol/request/verbs.rs`, `.github/rust-file-size-baseline.txt`

- [ ] Add a constructor in the test module, `fn envelope(id: u64, body: Request) -> Envelope` with `deadline_ms: None`. Row 1 carries `deadline_ms: Some(5000)`: keep that one literal as written.
- [ ] Convert every other row. Keep every comment above a row.
- [ ] Run the test shape. The proof is that `request_wire_v9.snap` is untouched: `git status --porcelain crates/shep-core/src/protocol/request/snapshots/` prints nothing and no `.snap.new` exists.
- [ ] `wc -l` the file. At 1000 or under, delete its entry and the two comment lines above it from the baseline file.
- [ ] Commit: `test(core): build the request wire fixtures with one constructor`

### Task 2: `DogTable`

**Files:** create `crates/shep-core/src/config/app/dog_table.rs`. Modify `config/app/mod.rs` and whichever `config/mod.rs` re-export list names `AppConfig`.

- [ ] Write the failing tests first, in the new file:
  - TOML through `toml::from_str` into a small struct holding a `DogTable`: `start = 09:00:00`, `day = 2026-09-27`, `at = 2026-09-27T09:00:00Z`, one inside a nested table and one inside an array. Each comes out as the plain JSON string. toml 0.8.23 hands these over as `{"$__toml_private_datetime":"09:00:00"}`, measured.
  - A JSON object with a real key `$__toml_private_datetime` beside a second key is left alone. Only the exact one-key shape converts.
  - `5`, `"x"` and `[1]` are refused.
  - `format!("{:?}", table)` is exactly `DogTable(<3 keys>)` and `DogTable(<1 key>)`, with a comment that a derived `Debug` would print a credential (IR-41).
  - A JSON round trip is byte-identical.
- [ ] Implement. `Deserialize` goes through `serde_json::Map` and walks it. Name toml's key in a `const` with a one-line comment saying it is toml_datetime's private field name.
- [ ] Under the `schema` feature, the schema is an object with `additionalProperties: true`. Implement `JsonSchema` by hand or with `schemars(with = ...)`, whichever the crate already does for a map type.
- [ ] Commit: `feat(core): add DogTable, one dog's opaque table on a sheep`

### Task 3: `AppConfig.dogs`, and the protocol moves to 10

Two commits, lookout first, because it is independent and small.

**Commit 1, files:** `crates/shep-cli/src/lookout/field/schema.rs`

- [ ] In `kind_of`, an object whose `additionalProperties` is itself an object schema (not `{type: string}`) is `FieldKind::Opaque`, not `FieldKind::Map`. `env` stays `Map`. Test with a synthetic schema beside the existing `env` test there.
- [ ] Commit: `fix(lookout): show a map of tables as read-only JSON`

**Commit 2, files:** `config/app/schema.rs` (field), `config/app/behavior.rs` (`Default`), `config/apply.rs` (`FIELDS`), `config/normalize/validate.rs` and `normalize/error.rs`, `config/scaffold.rs` (line-count test), `crates/shep-core/assets/flockfile.schema.json`, `protocol/mod.rs`, the three `*_wire_v9` snapshots, `web/src/pages/docs/first-flockfile.astro` (the scaffold line figure)

- [ ] Failing tests first:
  - `Flockfile::parse` accepts `[app.dogs.jobs]` in TOML, and the same shape in YAML, JSON and JSON5.
  - `dogs.jobs = 5` inside an app is refused. A top-level `[dogs.jobs]` is refused as an unknown key. Assert the error names `dogs`.
  - `normalize` refuses an empty dog name with a new `NormalizeError` variant whose doc states the condition.
  - `format!("{:?}", config)` for a config carrying a table with a value `super-secret-token` does not contain that string.
  - `apply_group("dogs") == ApplyGroup::Live`.
- [ ] The field, placed after `level_rules`, with an `init` extension: group `inputs`, a blurb, an example of `{ jobs = { concurrency = 2 } }`, `accepts` and `refuses`. Its `///` says shep stores it, never reads it, and hands each table to the dog it names.
- [ ] `("dogs", ApplyGroup::Live)` in `FIELDS`, with a comment beside `level_rules`'s: never read by the daemon.
- [ ] `PROTOCOL_VERSION` to 10, and its pin test. `MIN_SUPPORTED` stays 8. Rename `request_wire_v9`, `reply_wire_v9` and `bus_event_wire_v9` to `_v10`, snapshot files included, the way `a8338ceb` did for 8 to 9. Every serialized `AppConfig` in them gains `"dogs": {}`. Review the diff of each `.snap`: nothing else may move.
- [ ] Regenerate the schema asset:
  ```bash
  cargo run --bin shep -- schema > crates/shep-core/assets/flockfile.schema.json
  ```
- [ ] Fix the scaffold line-count test and the figure it names in `first-flockfile.astro`. Check `shep init --all` output in TOML, YAML and JSON5 uncomments into a Flockfile that parses (the existing test does this).
- [ ] Whatever else the full test shape turns red across the workspace is fallout of the new field: a field list, a lookout group test, a pm2 importer fixture. Fix each where it lives, and list them in the report.
- [ ] Commit: `feat(core): a sheep carries a settings table per dog`. The body says PROTOCOL_VERSION 9 to 10 and why MIN_SUPPORTED stays.

### Task 4: The wire

**Files:** `protocol/request/verbs.rs`, `protocol/request/response.rs`, `protocol/events.rs`, `protocol/mod.rs` re-exports if variants need types exported, and any exhaustive match the compiler names in other crates.

- [ ] The two requests, the two responses and the event from the contract. Docs on each: what it answers, `NotFound` and `IsADog` for the write, an empty map (never `NotFound`) for the read, and that a table's values never print.
- [ ] `BusEvent::topic` builds `config.sheep.{dog}` the way `DogConfigChanged` builds `config.dog.{dog}`. Test it beside `a_dog_config_event_names_the_dog_in_its_topic`, and that `config.*` still reaches it.
- [ ] One snapshot row for each new request, response and event in the `_v10` fixtures. A table in them holds two keys, one nested.
- [ ] Other crates: a new variant in a `#[non_exhaustive]` enum should hit existing wildcard arms. Where the daemon's dispatch has a wildcard that answers "unsupported", leave it for now: Task 5 adds the arms. Build must stay green at this commit.
- [ ] Commit: `feat(core): requests and an event for a dog's per-sheep tables`

**Review after bucket A:** one sonnet-high reviewer, spec plus quality plus empirical. Give it the spec, this plan's Tasks 1 to 4 and the contract, the range `git log main..HEAD`, and the two cargo commands to run.

---

### Task 5: Read and write in the daemon

**Files:** create `crates/shep-daemon/src/supervisor/actor_dog_tables.rs` and `crates/shep-daemon/src/rpc/tests/dog_tables.rs`. Modify `supervisor/actor_pane.rs`, `supervisor/command.rs`, `supervisor/handle.rs`, `supervisor/actor_core.rs` (`handle_command` arms), `supervisor/mod.rs` (module line), `rpc/dispatch.rs`, `rpc/tests/mod.rs` (module line).

- [ ] Failing rpc tests first, in the new test file, using the harness `rpc/tests/dog_fields.rs` uses:
  - `DogSheepSettings { dog: "jobs" }` answers the table of every sheep carrying one, skips a sheep carrying only another dog's, and answers an empty map for a dog nobody names.
  - `SetSheepDogSettings` sets, replaces, and removes (`table: None`) one table. A second dog's table on the same sheep is untouched each time.
  - It records an override of `dogs` (`overrides::get` shows the field, `SheepConfig.overridden` names it) and records to the muster roll. Assert the roll the way the `SetSheepField` tests do.
  - It answers `NotFound` for an unknown sheep and refuses a dog's own name the way `SetSheepField` does.
  - `SetSheepField` with key `dogs` is refused and the message names `SetSheepDogSettings`.
  - `ApplyConfig`: a plain load of a changed table reports `dogs` pending and leaves the stored table; `ResetDepth::File` applies it; `ResetDepth::Env` does not.
- [ ] Split `handle_set_sheep_field` (anchor: `pub(super) fn handle_set_sheep_field` in `actor_pane.rs`) into the door, which keeps the `env`, Structural and now `dogs` refusals, and a shared body taking `key` and `value` that everything after the refusals moves into. `actor_pane.rs` must not grow.
- [ ] The new handlers in `actor_dog_tables.rs`:
  - The read: every non-dog name, first slot per name, `spec.config().dogs.get(dog)`.
  - The write: builds the whole new `dogs` map from the intended config (`pending` if there is one, else the spec), then calls the shared body with `"dogs"` and that map as JSON.
- [ ] `Command` variants, `SupervisorHandle` methods, and `rpc/dispatch.rs` arms. The write's arm records to the registry exactly as the `SetSheepField` arm does, and says why in one line.
- [ ] Commit: `feat(daemon): read and write a dog's table on a sheep`

### Task 6: `config.sheep.<dog>`

**Files:** `actor_dog_tables.rs`, `supervisor/mod.rs` (a field on `Actor`), `supervisor/builder.rs` (seed it), `supervisor/actor_core.rs` (`run`), `rpc/tests/dog_tables.rs`

- [ ] Failing tests first:
  - A pure function in `actor_dog_tables.rs`, compared on two indexes, returns each `(dog, sheep)` whose table was added, removed or changed, once each, in a stable order. Unit tests in that file cover all three and "nothing changed".
  - Through rpc with a bus subscription on `config.sheep.jobs`: a `Start` of a sheep carrying a `jobs` table, a `SetSheepDogSettings` that changes it, one that writes the same table again (no event), a removal, and a `Delete` of the sheep. Each asserts the exact events received, bounded by a timeout (IR-46). A sheep carrying only a `deploy` table produces nothing on `config.sheep.jobs`.
  - A scale up of a sheep carrying a table fires nothing: the name already carried it.
- [ ] The actor holds the last index it announced (sheep name to that sheep's `dogs` map). After every message in `run`, it compares the current flock against it and sends one `DogSheepSettingsChanged` per change on `self.events`. Compare before cloning: a message that changed nothing must not clone a table.
- [ ] Seed the index wherever the builder installs a restored or handed-over flock, so a handover fires nothing. If restored entries arrive through commands instead, say so in the report: the spec's handover line is the one to correct, not the code to contort.
- [ ] Commit: `feat(daemon): announce a changed per-sheep table on config.sheep.<dog>`

### Task 7: End to end

**Files:** create `crates/shep-daemon/tests/daemon_e2e/dog_tables.rs`; add its `mod` line to `daemon_e2e/main.rs`. Model it on `daemon_e2e/smit.rs`.

- [ ] One test: a real daemon, a client subscribed to `config.sheep.jobs`, a `Start` carrying `[app.dogs.jobs]` with a nested table, then `DogSheepSettings`, then `SetSheepDogSettings`. Assert both events and both answers, each wait bounded.
- [ ] Run the e2e tier once for this file:
  ```bash
  cargo test --workspace --all-features --test daemon_e2e dog_tables
  ```
  This is the one extra cargo shape in the plan, run once, after the unit shape is green.
- [ ] Commit: `test(daemon): a dog hears its per-sheep table change end to end`

**Review after bucket B:** one sonnet-high reviewer with three lenses named: does the event fire from every path the spec lists and only those; can a write lose an operator's override or another dog's table; does anything print a table's values. It runs the unit shape and the e2e file.

---

### Task 8: The client

**Before starting:** `git fetch` and check whether #614 has merged (`gh pr view 614 --repo shep-pm/shep --json state`). If it has, rebase this branch onto `origin/main` first and re-run the unit shape.

**Files:** with #614 merged, a new `crates/shep-client/src/dogs/sheep_settings.rs` beside its `section.rs`, plus its typed-request file. Without it, `crates/shep-client/src/dogs.rs`.

- [ ] Failing tests first, modelled on #614's `a_refusal_names_the_line_and_never_the_value`: a table that fits parses; a table with a credential written into a wrongly typed field and into an unknown key (with `deny_unknown_fields` on the target) is refused, and neither `Display` nor `Debug` of the error contains the credential. `Display` is exactly the contract's sentence.
- [ ] `parse_sheep_settings` and `SheepSettingsError` from the contract. `impl core::error::Error`, no `source()` (the source is what quotes). If #614 merged, add `exit_code()` returning `INVALID_CONFIG` like `SectionError`.
- [ ] With #614 merged: a typed read on `Client` and `ReconnectingClient` following its pattern, answering `BTreeMap<String, DogTable>`. A daemon that answers the request as unrecognized maps to an error whose message says the shepherd predates per-sheep tables. Without #614: skip the typed read and put a `no_run` example on `parse_sheep_settings` that sends `Request::DogSheepSettings` through `Client::request`. Say which in the report.
- [ ] Commit: `feat(client): parse a sheep's table into a dog's settings type`

### Task 9: Docs

**Files:** `web/src/pages/docs/writing-a-dog.astro`, `dogs.astro`, `overrides.astro`, and `first-flockfile.astro` if its field table lists fields. `docs/decisions.md`, `docs/history.md`.

- [ ] Run `humanizer` then `rin-voice` over every sentence. No em dashes, no bold for emphasis.
- [ ] `writing-a-dog`: reading tables (`DogSheepSettings`, the parse helper), the `config.sheep.<dog>` topic, and that shep never validates a table. The `x-shep-sheep` schema key is PR 2's and stays out.
- [ ] `dogs`: three places a dog's config can live and which one to use. `dogs.toml` is the dog's own settings; `[app.dogs.<dog>]` is per sheep; the top-level `[dog.<name>]` is read and discarded by shep and exists for a dog that reads the Flockfile itself.
- [ ] `overrides`: `dogs` is one field. A changed table needs `--reset=file`.
- [ ] `docs/decisions.md`: one entry in the file's own shape, a heading, the decision, `**Why:**`, and the `verified` line naming the files read. Cover the `dogs` key over `dog`, one field over per-dog merge, the key in `--schema` over a new flag.
- [ ] `docs/history.md`: one line under the current section.
- [ ] Hard trigger, in order:
  ```bash
  cargo build --release
  ```
  ```bash
  ./web/scripts/generate-cli-reference.sh
  ```
  No verb changed, so expect no diff; commit one if there is. Then from `web/`: `npm ci`, `npx astro check`, `npm run build`. Never pipe `astro check` through `tail`.
- [ ] Commit: `docs(dogs): per-sheep tables for dog authors and operators`

### Task 10: Gate and PR (main thread)

- [ ] The task gate from `CLAUDE.md`: fmt, clippy, bare `cargo test --workspace --all-features`, and the rustdoc command.
- [ ] Cross-target checks for Linux and Windows.
- [ ] `git diff main --stat` and confirm no baselined file grew.
- [ ] Push, open the PR. Body per `rin-voice`: bullets, `Resolves #623` is wrong for PR 1 (the pane is still to come), so write `Part of #623`. Name every file over 500 lines that grew and why it was not split (an enum or a dispatch match cannot).
