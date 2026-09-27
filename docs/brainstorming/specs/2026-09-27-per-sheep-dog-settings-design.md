# Design: a sheep carries a settings table per dog

Status: designed 2026-09-27, not yet implemented. Resolves #623.

## The problem

A dog that acts per sheep has nowhere to keep per-sheep settings. A dog that
runs jobs per project wants each project sheep to carry its own working
hours, concurrency and merge policy. Today it keeps its own file keyed by
sheep name and keeps that in step with the flock by hand: a renamed or
deleted sheep leaves a stale entry, and a new one has none until someone
writes it.

The fix is a table on the sheep's own entry, which shep stores and hands to
the named dog without reading it.

## What already exists

Established from the code, not assumed.

- **The Flockfile already has a `[dog.<name>]` table, at the top level.**
  `RawFlockfile::dog` (`config/flockfile/raw.rs`) is read and discarded. It
  exists so shep-deploy's per-project build command does not get the whole
  file refused. It is per file, not per sheep, and nothing stores it.
- **`AppConfig` rides the wire, and every field change bumps the protocol.**
  `PROTOCOL_VERSION`'s own doc: an older peer ignores a key it does not know
  and runs a config the operator did not write. `environment` forced 8 and is
  the precedent. `MIN_SUPPORTED` is untouched by an additive field.
- **Overrides store whole fields as JSON.** `AppOverrides::fields` is a flat
  `serde_json::Map`, so a new field needs no store change. `env` is the only
  field merged per key, and that merge carries tombstones and a documented gap
  (`config_merge.rs`, `establish_env`).
- **`level_rules` is the precedent for a field the daemon never reads.** It is
  `ApplyGroup::Live` in `config/apply.rs` because it only rides out to a
  client.
- **`SetSheepField` already refuses `env`** and points at `SetSheepEnv`
  (`supervisor/actor_pane.rs`), because a whole-field write from a pane would
  wipe keys it was never shown.
- **`AppConfig`'s `Debug` is manual** (`config/app/behavior.rs`) and prints
  `name`, `script` and the env count, then `finish_non_exhaustive`. Any new
  field is already omitted.
- **A dog's config schema is asked fresh from its binary** with `--schema` and
  never stored (dog-config spec, decision 7). Secret fields carry
  `x-shep-secret` (`shep_core::dogs::SECRET_KEY`). The dog-config spec left
  validating a dog's config against that schema out of scope: "the dog is the
  authority on its own config and a shep that disagreed would be wrong in the
  direction that breaks a working dog."
- **Flockfile TOML goes through `serde_json::Value`** in `parse_declared`. A
  TOML datetime, date or time (`start = 09:00:00`, unquoted) arrives there as
  toml's private stand-in, `{"$__toml_private_datetime":"09:00:00"}`, nested
  tables included. Measured against toml 0.8.23, the version the workspace
  pins.
- **`verbs.rs` is 1,004 lines** and baselined in
  `.github/rust-file-size-baseline.txt`, which lets it shrink and never grow.
  Both new requests belong in it.

## Decisions

### 1. `dogs` is a field on `AppConfig`

```toml
[[app]]
name = "shop-api"
script = "./server"

[app.dogs.jobs]
concurrency = 2
hours = { start = 09:00:00, end = 17:00:00 }
merge = "ask"
```

The key is `dogs`, keyed by dog name. `dog` was the other candidate and
matches the top-level table, but a header missing its `app.` prefix,
`[dog.jobs]`, would land in that table and be silently discarded. With `dogs`
the same slip is an unknown key and fails loudly.

The type is `BTreeMap<String, DogTable>`, where `DogTable` is a
`serde(transparent)` newtype over `serde_json::Map<String, Value>`. Every
value must be a table: `dogs.jobs = 5` is refused in a file and on the wire.
A dog name must be non-empty and is otherwise not checked, against a grammar
or against the dogs this shepherd knows. Configuring a dog before installing
it is the order an operator wants.

Every instance of an app shares its table: config is per app.

`PROTOCOL_VERSION` goes from 9 to 10. `MIN_SUPPORTED` stays at 8.

### 2. shep checks the shape and reads nothing inside

The table must be a table. That is the whole check. No schema validation, for
the dog-config spec's reason quoted above.

One conversion happens: a TOML datetime, date or time becomes its RFC 3339
string. JSON has no such type, and the alternative is handing a dog toml's
private stand-in key, which a dog written against the JSON wire has no reason
to expect. Working hours are the issue's own example and the most likely
place for an unquoted `09:00:00`. YAML and JSON5 have no datetime type to
convert.

### 3. One field for every merge rule

`dogs` merges, overrides, drifts and resets like `level_rules`. Once a load
or an operator has established it, a plain `shep start Flockfile.toml` that
changes any dog's table reports it pending, and `--reset=file` applies it.
`--reset=env` leaves it alone. Per-dog merging like `env` was considered and
left out: it would copy env's tombstones and gap for a case, adding a second
dog to one sheep, that nobody has asked for yet.

`ApplyGroup::Live`, beside `level_rules`: the daemon never reads it, so a
write is in force at once and nothing respawns or re-arms.

### 4. A dog reads its tables with one request

`Request::DogSheepSettings { dog }` answers
`Response::DogSheepSettings { tables }`, a `BTreeMap<String, DogTable>` from
sheep name to that dog's table, for every sheep carrying one. A dog never sees
another dog's tables. No sheep carrying one is an empty map, never
`NotFound`. Dogs carry no tables and never appear.

It reads the stored spec, which is what is in force. `dogs` is Live, so it is
parked only when a whole config fails to normalize and parks with it.

`SheepConfig` carries `dogs` too, because it carries the whole `AppConfig`.
That is lookout's read, not a dog's.

`dogs` does not go on `ProcessInfo`. `shep flock --json` and `describe` do not
change, and `SCHEMA_VERSION` stays where it is.

### 5. One write, one dog's table on one sheep

`Request::SetSheepDogSettings { name, dog, table: Option<DogTable> }` replaces
one dog's table, or removes it with `None`. It answers
`Response::SheepDogSettingsSet { name, dog }`, `NotFound` for an unknown
sheep, and `IsADog` for a dog's own name, as `SetSheepField` does.

The daemon builds the new `dogs` map from the intended config and records it
as an operator override of the whole `dogs` field, so the `*` marker shows.
It is recorded to the muster roll for `SetSheepField`'s stated reason: the
restore path never reads the override store.

`SetSheepField` refuses the key `dogs` and names this request, the way it
refuses `env`. A whole-map write from a pane editing one dog would overwrite
a concurrent edit to another dog's table.

### 6. A dog hears about a change on its own topic

`BusEvent::DogSheepSettingsChanged { dog, sheep }`, topic
`config.sheep.<dog>`. It fires whenever the answer to
`DogSheepSettings { dog }` changes, once per dog and sheep per change:

- a name carrying tables is registered (`Start`, `Add`, a muster)
- the last instance of such a name is deleted
- a load, `SetSheepDogSettings` or `--reset` changes, adds or removes a table

A handover changes no answer and fires nothing. A dog re-reads when it
connects in any case. New variants are free under the compatibility rules.

The event says only what changed, like `config.dog.<name>`. The dog re-reads
all its tables or just that sheep's.

### 7. Nothing prints a table's values

A per-sheep schema can mark a field `x-shep-secret` (decision 9), so a table
can hold a credential. `DogTable`'s `Debug` prints `DogTable(<3 keys>)`, with
an exact-string test (IR-41). `AppConfig`'s `Debug` already omits the field.

`SheepConfig` carries tables whole, and the pane masks fields marked secret.
That is the exposure `DogConfig`'s section has today, behind the same `0700`
socket.

### 8. The client parses a table into the dog's own type

`shep-client` gains a typed read that returns every table for a dog, and
`DogTable::parse::<S>()` in shep-core that deserializes one into the dog's
type. A dog parses each sheep's table on its own, so one bad table does not
hide the rest.

The parse error names the sheep's key path that failed and never carries the
parser's message, which can quote a value. #614 makes the same call for a
dog's section, and this follows whatever shape that lands in.

An older daemon answers the new request as unrecognized. The typed read maps
that to an error that says the shepherd predates per-sheep tables.

### 9. A dog publishes a per-sheep schema inside `--schema`

The existing `--schema` answer gains one key, `x-shep-sheep`, holding the
schema for the dog's per-sheep table. It shares the root's `$defs`, so one
`SchemaGenerator` produces both halves and a `$ref` resolves the way every
other one in the document does.

```json
{
  "title": "JobsConfig",
  "properties": { "...": "the dogs.toml section" },
  "x-shep-sheep": { "$ref": "#/$defs/ProjectSettings" },
  "$defs": { "ProjectSettings": { "...": "the [app.dogs.jobs] table" } }
}
```

A key rather than a new flag: no extra spawn each time a pane opens, and a
dog that predates it simply has no key. It follows `x-shep-secret`, the other
extension shep reads out of that answer. `SHEEP_SCHEMA_KEY` joins
`SECRET_KEY` in `shep_core::dogs`.

Dog side, beside `probe::<T>()`: `probe_with_sheep::<T, S>()` and
`config_schema_with_sheep::<T, S>()`. `S` is bound by `DogConfig`, so
`#[shep(secret)]` marks its fields exactly as it marks `T`'s. The built-in
dogs publish no per-sheep schema.

### 10. Lookout edits a table through that schema

The sheep pane's `dogs` row opens a sub-screen. It lists every known dog whose
schema carries `x-shep-sheep`, marking which have a table on this sheep, plus
any dog with a table here and no such schema.

```
shop-api › dogs

  jobs      set   concurrency=2 merge=ask
  deploy    -     (no table)
  legacy    set   read-only: publishes no sheep schema

  enter edit   d remove   esc back
```

- Opening it probes every adopted dog in parallel, off the UI task, each under
  `VERSION_BUDGET`, as the dog config pane already probes one. Built-in dogs
  are free.
- Enter opens a schema-driven pane on that dog's table, built the way the dog
  config pane builds from `--schema`. Secret fields show `<set>`. Closing
  writes through `SetSheepDogSettings`.
- `d` removes the table after a confirmation, since it is destructive.
- A table for a dog with no sheep schema, or no binary, is shown as read-only
  JSON.

Without the sub-screen, the pane's `kind_of` would class `dogs` as env's
string-map editor, since it is an object with `additionalProperties`. A map
whose values are objects is not that, and `kind_of` learns so.

## Out of scope

- **Validating a table.** Decision 2.
- **A CLI verb that writes a table.** The Flockfile and lookout are the two
  doors. A `SetSheepDogSettings` from a script is available to anyone who
  wants one.
- **Showing tables in `shep describe`.** Possible later without a wire change,
  through `SheepConfig`.
- **The top-level `[dog.<name>]` table.** Unchanged. shep-deploy can move to a
  per-sheep table in its own repo when it wants to.
- **Per-sheep schemas for bark and metrics.** Neither has a per-sheep setting.

## Testing

PR 1:

- A Flockfile with `[app.dogs.jobs]` parses in all four formats. `dogs.jobs = 5`
  is refused naming the key. `[dogs.jobs]` at the top level is refused as an
  unknown key.
- A TOML datetime, date and time inside a table reach the stored config as
  RFC 3339 strings, nested ones included.
- A plain load of a changed table reports `dogs` pending, `--reset=file`
  applies it, `--reset=env` does not.
- `DogSheepSettings` answers only that dog's tables, an empty map for a dog
  nobody names, and nothing for a dog's own entry.
- `SetSheepDogSettings` sets, replaces and removes one table and leaves another
  dog's table on the same sheep untouched. It records an override, survives a
  cold restart through the muster roll, refuses a dog's name and answers
  `NotFound` for an unknown sheep. `SetSheepField` refuses `dogs`.
- The event fires once per dog and sheep for each trigger in decision 6, and
  not for a write that leaves a table as it was. One e2e test subscribes to
  `config.sheep.jobs` and watches a load and a write arrive.
- `DogTable`'s `Debug` exact string. `AppConfig`'s `Debug` of a config
  carrying a table does not contain its values.
- `DogTable::parse` into a dog's type, and a failure whose message has no
  value in it.
- Wire fixtures for both requests, both responses and the event, and the
  protocol-version pin at 10.
- The Flockfile JSON Schema asset is regenerated and its test passes.

PR 2:

- `x-shep-sheep` round-trips through `probe_with_sheep`, with a secret field
  in `S` marked. A dog with no key gets no editable row.
- The sub-screen's list, edit, remove and read-only paths, on captured frames.

## Documentation

- PR 1: `writing-a-dog` (reading tables, the event, the typed read), `dogs`
  (per-sheep tables against the `dogs.toml` section and the top-level table),
  `overrides` (`dogs` is one field), the Flockfile field in `first-flockfile`
  if its field table lists them, `docs/decisions.md`, `docs/history.md`.
- PR 2: `lookout` (the sub-screen) and `writing-a-dog` (publishing
  `x-shep-sheep`).

## Delivery

Two pull requests.

1. This spec, PR 1's plan, and everything but the pane: shep-core,
   shep-daemon, shep-client and their docs. A dog can read and receive its
   tables once this merges.
2. The per-sheep schema key, `probe_with_sheep`, and the lookout sub-screen.
   Its plan is written when PR 1 is finishing, against the code as merged.

PR 1 starts with a pure reduction of `verbs.rs`: `request_wire_snapshots`
builds 34 `Envelope` literals by hand, and a small constructor cuts about 100
lines. The `.snap` file stays byte-identical, which is the proof nothing
changed, and the baseline entry is deleted. The enum stays whole.

PR 2 touches files baselined over 1000 lines (`sheep_pane.rs`, `update.rs`,
`selection.rs`). New behaviour goes in new files and the baselined ones do not
grow.

#614 moves `shep_client::dogs` into a directory and adds typed requests. If
it has merged when PR 1's client task runs, that task follows its layout.
