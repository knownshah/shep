//! Answering shep's probes: the version flag and the schema flag.

use std::io::Write as _;

use shep_core::dogs::{SCHEMA_FLAG, SHEP_PROTOCOL_KEY, VERSION_FLAG};

use super::DogConfig;
#[cfg(feature = "schema")]
use super::{config_schema, config_schema_with_sheep};

/// Answers shep's probes, and returns when this run is not a probe, so a
/// dog calls it as the first line of `main`.
///
/// `name` and `version` are ordinarily `env!("CARGO_PKG_NAME")` and
/// `env!("CARGO_PKG_VERSION")`; only the version is read by shep.
///
/// # Exits
///
/// Ends the process with [`process::exit`](std::process::exit), status 0,
/// before `main` opens anything.
#[cfg(feature = "schema")]
pub fn probe<T: DogConfig + schemars::JsonSchema>(name: &str, version: &str) {
    probe_answering(name, version, schema_answer::<T>);
}

/// [`probe`], for a dog that also publishes a schema for its per-sheep
/// `[app.dogs.<name>]` table: the schema flag answers with
/// [`config_schema_with_sheep::<T, S>`](config_schema_with_sheep) instead of
/// [`config_schema::<T>`](config_schema).
///
/// # Exits
///
/// Ends the process with [`process::exit`](std::process::exit), status 0,
/// before `main` opens anything.
///
/// # Examples
///
/// ```no_run
/// # #[shep_client::dogs::dog_config]
/// # #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
/// # struct MyDogConfig {}
/// # #[shep_client::dogs::dog_config]
/// # #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
/// # struct MySheepSettings {}
/// fn main() {
///     shep_client::dogs::probe_with_sheep::<MyDogConfig, MySheepSettings>(
///         env!("CARGO_PKG_NAME"),
///         env!("CARGO_PKG_VERSION"),
///     );
///     // ...normal startup, reached only when this run is not a probe.
/// }
/// ```
#[cfg(feature = "schema")]
pub fn probe_with_sheep<T, S>(name: &str, version: &str)
where
    T: DogConfig + schemars::JsonSchema,
    S: DogConfig + schemars::JsonSchema,
{
    probe_answering(name, version, schema_answer_with_sheep::<T, S>);
}

/// The body both probes share: answers the version or the schema flag,
/// the schema rendered by `schema` only when it is asked for.
#[cfg(feature = "schema")]
fn probe_answering(name: &str, version: &str, schema: fn() -> String) {
    match first_argument().as_deref() {
        Some(VERSION_FLAG) => answer(&version_answer(name, version)),
        Some(SCHEMA_FLAG) => answer(&schema()),
        _ => (),
    }
}

/// Answers shep's probes, and returns when this run is not a probe, so a
/// dog calls it as the first line of `main`.
///
/// `name` and `version` are ordinarily `env!("CARGO_PKG_NAME")` and
/// `env!("CARGO_PKG_VERSION")`; only the version is read by shep. With the
/// `schema` feature off, the schema flag exits without printing, so shep
/// records a dog with no schema instead of waiting out a timeout.
///
/// # Exits
///
/// Ends the process with [`process::exit`](std::process::exit), status 0,
/// before `main` opens anything.
#[cfg(not(feature = "schema"))]
pub fn probe<T: DogConfig>(name: &str, version: &str) {
    match first_argument().as_deref() {
        Some(VERSION_FLAG) => answer(&version_answer(name, version)),
        Some(SCHEMA_FLAG) => std::process::exit(0),
        _ => (),
    }
}

/// [`probe_with_sheep`] with the `schema` feature off: behaves exactly like
/// [`probe`] does without the feature, since there is no schema to answer
/// with either way.
///
/// # Exits
///
/// Ends the process with [`process::exit`](std::process::exit), status 0,
/// before `main` opens anything.
#[cfg(not(feature = "schema"))]
pub fn probe_with_sheep<T: DogConfig, S: DogConfig>(name: &str, version: &str) {
    probe::<T>(name, version);
}

/// The argument shep spawns a probe with, which is the only one it passes.
///
/// Only the first: `docs/dogs.md` publishes that contract, and a dog's
/// own arguments are its business.
fn first_argument() -> Option<String> {
    std::env::args().nth(1)
}

/// Prints an answer and ends the process.
///
/// The explicit flush matters: [`std::process::exit`] runs no destructor,
/// so nothing else would push a partial buffer out.
fn answer(text: &str) -> ! {
    let mut stdout = std::io::stdout();
    // A write to a closed stdout is not something a dog can do anything
    // about, and shep reads it as silence, which is a legal answer.
    let _ = write!(stdout, "{text}");
    let _ = stdout.flush();
    std::process::exit(0);
}

/// The `--version` answer, whole, ending in a newline.
///
/// Split out from [`probe`] so the format can be tested against
/// [`shep_core::dogs::parse_version_answer`], the code that reads it.
fn version_answer(name: &str, version: &str) -> String {
    format!(
        "{name} {version}\n{SHEP_PROTOCOL_KEY}: {}\n",
        crate::PROTOCOL_VERSION
    )
}

/// The `--schema` answer, whole, ending in a newline.
#[cfg(feature = "schema")]
fn schema_answer<T: DogConfig + schemars::JsonSchema>() -> String {
    rendered(&config_schema::<T>())
}

/// `schema` as the schema flag prints it: pretty JSON and a newline.
#[cfg(feature = "schema")]
fn rendered(schema: &schemars::Schema) -> String {
    // The same expectation shep-core's own schema printer holds: a schemars
    // `Schema` is a `serde_json::Value` already, so serializing it cannot
    // meet a type serde_json has no representation for.
    let json = serde_json::to_string_pretty(schema).expect("a schemars Schema always serializes");
    format!("{json}\n")
}

/// The `--schema` answer for [`probe_with_sheep`], whole, ending in a
/// newline.
#[cfg(feature = "schema")]
fn schema_answer_with_sheep<T, S>() -> String
where
    T: DogConfig + schemars::JsonSchema,
    S: DogConfig + schemars::JsonSchema,
{
    rendered(&config_schema_with_sheep::<T, S>())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two ends of the grammar: what a dog prints and what shep reads.
    /// Pinned as a round trip rather than as a string, because the format is
    /// only ever interesting to the parser.
    #[test]
    fn the_version_answer_parses_with_the_shepherds_own_parser() {
        let answer = version_answer("shep-otel", "0.1.3");
        let parsed = shep_core::dogs::parse_version_answer(&answer)
            .expect("the answer shep's own parser cannot read is the bug this pins");

        assert_eq!(parsed.version, "0.1.3");
        assert_eq!(parsed.protocol, Some(crate::PROTOCOL_VERSION));
    }
}
