# Per-sheep dog settings, PR 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A dog publishes a schema for its per-sheep table inside its `--schema` answer, and lookout edits a sheep's `[app.dogs.<dog>]` table through it, nested tables included.

**Architecture:** One shep-core constant and two shep-client functions put an `x-shep-sheep` key into the existing schema answer. In lookout, the sheep pane's `dogs` row opens a sub-screen listing dogs, and Enter opens a `ConfigPane` over one dog's table on that sheep. Its rows come from the sheep schema, with a nested table's fields flattened into dotted rows. Its write is PR 1's `Request::SetSheepDogSettings`. No wire type changes. `PROTOCOL_VERSION` stays 10.

**Spec:** `docs/brainstorming/specs/2026-09-27-per-sheep-dog-settings-design.md`, decisions 9 and 10. PR 1 (#627) is merged at `c6ab2ccf`.

**Decided since the spec (maintainer, 2026-09-28):** a nested table in the per-sheep schema becomes dotted rows (`models.worker.model`), each editable by its type. Arrays and arrays of tables stay read-only. The dog config pane is not changed; a follow-up issue brings it along.

**Tech stack:** Rust 2024, MSRV 1.88, ratatui lookout, schemars 1.2, insta. No new dependencies.

## Global constraints

- **Invoke `rust-house-style` before writing Rust**, and read `docs/rust-house-style-addendum.md`.
- **One cargo shape for every task:**
  ```bash
  cargo test --workspace --lib --bins --all-features -- --skip ::slow::
  ```
  ```bash
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  ```
  plus `cargo fmt --all --check`. No `-p` runs. One cargo command at a time, `$?` captured directly.
- **Gate every commit** with those three. Commit the moment it compiles. Never `git checkout -- <file>` on uncommitted work.
- **Conventional commit subjects**, no `!`: nothing here breaks a published API. `probe` keeps its signature.
- **IR-48, and the local gate does not check it.** Before every commit that grows a file, run:
  ```bash
  git ls-files '*.rs' | xargs wc -l | awk '$1 > 1000 && $2 != "total"'
  ```
  and compare against `.github/rust-file-size-baseline.txt`. Baselined files this plan must not grow: `lookout/app/update.rs` (1134), `lookout/app/selection.rs`, `lookout/app/sheep_pane.rs`, `lookout/view/settings.rs`, `lookout/app/bleats.rs`. `verbs.rs` is at exactly 1000 and is not touched. New behaviour goes in new files.
- **Comments (IR-47):** only what the code cannot say, no history, no em dashes.
- **Secrets:** a table value never reaches a `Debug`, a notice, a log line or a screen row unless the schema says the field is not secret. Where there is no schema, show key names, never values.
- **Never name the private dog that motivated this.** Examples use `jobs`.
- **Every snippet about existing code is a guess with a grep anchor.** Follow the code, and say so in the report.
- **Every subagent brief states the commit rule in its own text.**

## Interface contract

```rust
// shep_core::dogs
pub const SHEEP_SCHEMA_KEY: &str = "x-shep-sheep";

// shep_client::dogs (with the `schema` feature)
pub fn config_schema_with_sheep<T, S>() -> schemars::Schema
where T: DogConfig + schemars::JsonSchema, S: DogConfig + schemars::JsonSchema;
pub fn probe_with_sheep<T, S>(name: &str, version: &str)
where T: DogConfig + schemars::JsonSchema, S: DogConfig + schemars::JsonSchema;
// Without `schema`: probe_with_sheep::<T, S> exists with `T: DogConfig, S: DogConfig`
// and behaves exactly like `probe::<T>` does without the feature.

// shep-cli lookout
PaneTarget::SheepDog { sheep: String, dog: String, adopted_path: Option<PathBuf> }
ConfigPane::sheep_dog(sheep, dog, adopted_path, schema: &Value, table: Map<String, Value>) -> Self
ConfigPane::edited_table_with(&self, edits: &Edits) -> Map<String, Value>
Effect::LoadSheepDogs { sheep: String }
Msg::SheepDogs { sheep: String, dogs: Vec<SheepDogEntry> }
Sent::SetSheepDogTable { name, dog, ticket, table: Option<DogTable>, authority }
```

`SheepDogEntry` holds a dog's name, its adopted path, and its sheep schema as `Option<Value>`: the `x-shep-sheep` value with the root's `$defs` attached, or `None`.

## Dispatch buckets

| bucket | tasks | model | why |
| --- | --- | --- | --- |
| A | 1 and 2 | sonnet, high | pure functions, no UI state |
| B | 3 to 6 | opus, medium | lookout state machine, writes to the live override store, secret display |
| C | 7 and 8 | sonnet, high | frames and docs |

Estimated wall-clock: A about 45 minutes, B about 100, C about 50, plus the main thread's TUI check and gate.

---

### Task 1: `x-shep-sheep` in the schema answer

**Files:** `crates/shep-core/src/dogs.rs`, `crates/shep-client/src/dogs.rs`. If #614 has merged by then, `crates/shep-client/src/dogs/mod.rs` and its layout instead: check with `gh pr view 614 --repo shep-pm/shep --json state` and rebase first.

- [ ] Failing tests first, beside the existing schema tests in shep-client:
  - `config_schema_with_sheep::<Section, Sheep>()` has `x-shep-sheep`, and that value's `$ref` resolves in the root's own `$defs`.
  - A `#[shep(secret)]` field in `Sheep` carries `SECRET_KEY` in the resolved sheep schema, exactly as one in `Section` does in the root.
  - A nested struct inside `Sheep` resolves too, so lookout can flatten it.
  - `config_schema::<T>()` is byte-identical to before: no `x-shep-sheep` key.
- [ ] `SHEEP_SCHEMA_KEY` beside `SECRET_KEY`, with a doc saying the value is the schema of a dog's `[app.dogs.<name>]` table.
- [ ] One `SchemaGenerator`: `subschema_for::<S>()` first, then `into_root_schema_for::<T>()`, then `insert(SHEEP_SCHEMA_KEY, sheep)`. That order puts `S`'s definitions in the root's `$defs`. Verified against schemars 1.2's `generate.rs` and `schema.rs`: all three exist with those names.
- [ ] `probe_with_sheep` answers `--version` like `probe` and `--schema` with the combined schema. The non-`schema` twin mirrors `probe`'s.
- [ ] The crate-level doc example stays on `probe`. `probe_with_sheep`'s own doc has one `no_run` example.
- [ ] Commit: `feat(client): a dog publishes its per-sheep table schema with probe_with_sheep`

### Task 2: Flatten a schema into dotted rows

**Files:** create `crates/shep-cli/src/lookout/field/flatten.rs`. Modify `lookout/field/mod.rs` (module line, re-export).

- [ ] Failing tests first, in the new file:
  - A property whose resolved schema is an object with `properties` becomes one row per leaf: `models` with `worker` with `model` becomes `models.worker.model`. Depth three at least, through `$ref`.
  - A leaf keeps its `FieldKind` (`Choice` for an enum, `Integer`, `Bool`, `Text`) and its `x-shep-secret` mark.
  - An array, an array of tables, and an object with `additionalProperties` stay one `Opaque` row, not editable.
  - Each row's path is the real key list. A schema property literally named `a.b` keeps the path `["a.b"]`, not `["a", "b"]`.
  - The same flatten over a table's values gives the display map keyed by the same dotted keys.
- [ ] `pub(crate) fn flattened(schema: &Value) -> Flattened`, where `Flattened` holds the `FieldSet` (no groups, schema order) and a `BTreeMap<String, Vec<String>>` from dotted key to path. Build leaves with the same per-field builder `FieldSet::from_properties` uses (`field_from`), so kinds and help come out identical.
- [ ] `pub(crate) fn flatten_values(table: &Map<String, Value>, paths: &BTreeMap<String, Vec<String>>) -> Map<String, Value>`.
- [ ] Commit: `feat(lookout): flatten a dog's per-sheep schema into dotted rows`

**Review after bucket A:** a `/q-review` round on the two new files, then the main thread verifies each finding. No Claude reviewer subagent: the delegation guard routes read-only review to Q.

---

### Task 3: A pane over one dog's table on one sheep

**Files:** create `crates/shep-cli/src/lookout/pane/sheep_dog.rs`. Modify `pane/types.rs` (`PaneTarget`), `pane/config.rs` (module wiring and a field for the flattened paths and the raw table), and every `match` on `PaneTarget` the compiler names: `pane/config.rs`, `pane/edit.rs`, `app/config_pane.rs`, `app/config_pane_edits/dispatch_writes.rs`, `view/pane/draw.rs`, `view/pane/chrome.rs`.

- [ ] Failing tests first, in the new file:
  - `ConfigPane::sheep_dog` over a schema with a nested table shows dotted rows with the table's values.
  - A secret field renders `<set>` and its value is in no rendered row.
  - `edited_table_with` applies a set at a nested path, removes a leaf on `null`, keeps every key no edit touched (including keys the schema does not name), and never touches a sibling table.
  - `ConfigPane`'s `Debug` of this pane contains no table value.
- [ ] `PaneTarget::SheepDog`. Its `name()` is the sheep's. `cost()` answers `None`, like a dog's: shep does not know what a dog does with a change.
- [ ] Chrome titles it `<sheep> › <dog>`.
- [ ] Commit: `feat(lookout): a config pane over one dog's table on a sheep`

### Task 4: The `dogs` row and its sub-screen state

**Files:** create `crates/shep-cli/src/lookout/pane/dogs.rs` (the `DogsPane` state: entries, cursor, one armed removal). Modify `pane/config.rs` (a `dogs: Option<DogsPane>` beside `list`, never open with it), `pane/config.rs`'s `value` (the `dogs` special case beside the `env` one).

- [ ] Failing tests first:
  - The sheep pane's `dogs` row renders dog names only: `jobs, deploy`, or `none`. Never a value. This replaces PR 1's compact JSON, which drew secret-marked fields.
  - `DogsPane::new` from the sheep's tables and a probe answer lists every dog whose schema has `x-shep-sheep`, marked set or unset, plus every dog with a table and no sheep schema, marked read-only. Sorted by name.
  - A read-only entry shows its table's key names, never values: without a schema nothing says which are secret.
  - Arming a removal on an unset entry does nothing.
- [ ] Commit: `feat(lookout): the dogs row lists a sheep's tables by dog name`

### Task 5: Wiring: open, probe, navigate, remove

**Files:** create `crates/shep-cli/src/lookout/app/pane_dogs.rs` (keys and replies for the sub-screen). Modify `app/msg.rs` (`Effect::LoadSheepDogs`, `Msg::SheepDogs`), `app/rows.rs` (`Sent::SetSheepDogTable` to `Request::SetSheepDogSettings`), `app/config_pane.rs` (route keys to the sub-screen when open, as it does for `list`), `app/config_pane_edits/confirm_field.rs` (Enter or `e` on the `dogs` row returns `Effect::LoadSheepDogs`), `ui_event_loop.rs` (run the probe), `commands/settings.rs` (make `dog_candidates` `pub(crate)`), `app/dog_pane.rs`, `app/update.rs`.

- [ ] **`update.rs` shrinks first, in its own commit.** Move the body of the `Msg::DogPane` arm into `App::on_dog_pane` in `dog_pane.rs` (anchor: `Msg::DogPane {` in `update.rs`). That frees about 15 lines. Commit: `refactor(lookout): the dog pane's probe answer is handled beside the pane`. Then every arm this task adds fits, and the file ends no longer than 1134.
- [ ] Failing tests first, using the app test harness in `app/testing.rs`:
  - Enter on the `dogs` row returns `Effect::LoadSheepDogs` for that sheep, and a `Msg::SheepDogs` reply opens the sub-screen. A reply for a sheep the pane has left opens nothing, as `on_dog_section` guards its reply.
  - `j`/`k` move, Esc closes the sub-screen and leaves the sheep pane open.
  - Enter on an entry with a sheep schema opens the `SheepDog` pane with the sheep's table from the values the sheep pane already holds. No second request.
  - Enter while the sheep pane holds unwritten edits refuses with a notice that names `esc`, and opens nothing.
  - `d` arms, Enter confirms and returns `Sent::SetSheepDogTable { table: None }`. Any other key disarms. A closed control gate refuses, in the gate's own words.
  - A `SheepDogSettingsSet` reply lands a notice and re-reads the sheep, so the sub-screen refreshes. An `Rpc` error lands its message. Neither prints a value.
- [ ] The probe: in `ui_event_loop.rs`, beside `Effect::LoadDogPane`, `spawn_blocking` reads `shep.toml`, takes `dog_candidates`, and asks each schema in parallel (`std::thread::scope`). A built-in comes from `crate::dog::builtin_schema`, an adopted dog from `commands::dogs::ask_schema` under `VERSION_BUDGET`. Keeps only `x-shep-sheep` with the root's `$defs`.
- [ ] Commit: `feat(lookout): open, probe and remove a sheep's dog tables`

### Task 6: The pane's write and refresh

**Files:** `app/config_pane_edits/dispatch_writes.rs`, `app/config_pane.rs` (`reread_pane`), `app/pane_dogs.rs`.

- [ ] Failing tests first:
  - Closing a `SheepDog` pane with edits returns one `Sent::SetSheepDogTable` carrying `edited_table_with`'s whole table, under the same `WriteAuthority` a sheep write takes. With no edits it sends nothing.
  - Esc lands on the dashboard, as every config pane does (`close_pane`'s doc gives the reason).
  - `r` re-reads the sheep and rebuilds from its current table, keeping the cursor.
- [ ] Commit: `feat(lookout): write one dog's table on a sheep from its pane`

**Review after bucket B:** `/q-review` on the new files and the hunks of modified ones. Then the main thread runs its own round, reading the secret-display paths and the write path end to end. It owns the security judgement.

---

### Task 7: Frames and a real screen

**Files:** `lookout/frames/` scenes and the view tests beside `view/pane/`, `docs/lookout/frames.{txt,ansi}`.

- [ ] Scenes: the sheep pane with the names-only `dogs` row; the sub-screen with a set, an unset and a read-only entry; the sub-screen with a removal armed; the `SheepDog` pane with dotted rows and a secret field. Add them to the gallery in the same commit, then run:
  ```bash
  cargo test --workspace --lib --bins --all-features -- --ignored write_the_gallery
  ```
  and read the diff of `docs/lookout/frames.txt` by hand.
- [ ] Commit: `test(lookout): frames for the per-sheep dog table screens`

### Task 8: Docs

**Files:** `web/src/pages/docs/lookout.astro`, `writing-a-dog.astro`, `first-flockfile.astro`, `docs/decisions.md`, `docs/history.md`.

- [ ] `humanizer` then `rin-voice` over every sentence.
- [ ] `writing-a-dog`: publishing a per-sheep schema with `probe_with_sheep`, and that a nested struct becomes dotted rows in lookout.
- [ ] `lookout`: the `dogs` row, the sub-screen and its keys, and the pane.
- [ ] `first-flockfile`: the `dogs` row and the field-table note stop saying lookout cannot edit it.
- [ ] `decisions.md`: dotted rows in this pane only, and names-only display where no schema says what is secret.
- [ ] Site gate from `web/`: `npm ci`, `npx astro check`, `npm run build`.
- [ ] Commit: `docs(dogs): editing a sheep's dog tables in lookout`

### Task 9: Real screen, gate, PR (main thread)

- [ ] A fake dog: a shell script answering `--schema` with a nested sheep schema and one secret field, adopted into a scratch `SHEP_HOME`. Drive lookout with the `tui-screen-capture` skill against the worktree's own binary: open the sheep, the sub-screen, the pane, edit a nested field, close, and confirm with `SheepConfig` that the table changed and the secret never drew.
- [ ] Task gate, cross-target checks, the IR-48 check above.
- [ ] File the follow-up issue: dotted rows for the dog config pane.
- [ ] Push, open the PR with `Resolves #623`.

## Assumptions for the maintainer to check

- Esc from a dog's table pane lands on the dashboard, not back on the sub-screen, as every config pane's Esc does.
- Enter on a dog refuses while the sheep pane holds unwritten edits. Esc writes or discards them first.
- Removing a table is armed by `d` and confirmed by Enter, the secrets pane's pattern.
- The spec said a table with no sheep schema shows as read-only JSON. It shows key names only, since nothing says which values are secret.
- The `dogs` row on the sheep pane shows names only from now on. PR 1 drew it as JSON, which included secret-marked values.
- Schemas are probed each time the sub-screen opens and never stored, per the dog-config spec.
