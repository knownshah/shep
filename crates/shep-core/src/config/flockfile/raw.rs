//! The shape serde accepts, before any validation.
//!
//! `deny_unknown_fields` is the point: a typo must fail loudly. `$schema`
//! and `dog` are let in beside `app` explicitly, one field at a time, so
//! the next one has to be added the same way.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::config::AppConfig;

use super::{error::FlockfileError, format::FlockFormat, parse::parse_into_ignoring};

// Application entries are locked to the `app` key: a typo'd key must fail
// loudly. `$schema` and `dog` are the two keys explicitly let in beside
// it; a future schema key is added the same explicit way, so older
// binaries reject newer Flockfiles by design rather than ignore them.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
// `rename` sets `schema_name`, which schemars uses as the root schema's
// `title`. The type is called `RawFlockfile` because it is the
// pre-validation twin of `Flockfile`; the document an operator writes is a
// Flockfile, and that is what the title has to say.
#[cfg_attr(feature = "schema", schemars(rename = "Flockfile"))]
#[serde(deny_unknown_fields)]
pub(super) struct RawFlockfile {
    /// The editor's schema hint, read and discarded.
    ///
    /// This is the "future schema key" the comment above anticipated, added
    /// HERE explicitly rather than by relaxing `deny_unknown_fields`: a
    /// typo'd key must still fail loudly, and exactly one more key is now
    /// legal. shep does not validate against the named schema and makes no
    /// promise about it — it is a hint for the operator's editor, which is
    /// the only consumer that ever reads it.
    ///
    /// TOML Flockfiles do not need it: taplo's `#:schema <url>` directive is
    /// a comment, invisible to serde. JSON and JSON5 have no comment an
    /// editor agrees to look in, which is why this field exists at all.
    #[serde(default, rename = "$schema")]
    pub(super) schema: Option<String>,
    /// A dog's own per-app configuration, read and discarded.
    ///
    /// Added the same way `$schema` was, explicitly rather than by relaxing
    /// `deny_unknown_fields`, so a typo'd key still fails loudly and exactly
    /// one more key is legal.
    ///
    /// It exists because the alternative is a Flockfile no daemon will accept.
    /// A dog that needs per-app configuration has nowhere to put it: shep-deploy
    /// wants a build command for the app it deploys, which belongs beside that
    /// app's declaration and nowhere else, and a Flockfile carrying one was
    /// refused outright by `shep start`. Measured 2026-08-28 against shep
    /// 0.1.8: an operator following shep-deploy's own README could not register
    /// their app at all. `unknown field `build`, expected `$schema` or `app``.
    ///
    /// It must BE a table. shep does not read what is inside it, does not
    /// validate it, and makes no promise about it. Those are two different
    /// claims and only the second one is a promise not to care: the dog that
    /// owns a key under this table is the only thing that understands it, and
    /// shep refusing a document because it does not recognise another
    /// program's config is a coupling neither side wants.
    ///
    /// Nested under one key rather than allowing loose top-level keys, so
    /// exactly one name is reserved and a typo anywhere else still fails.
    ///
    /// A map of ignored values rather than `IgnoredAny`, which would have
    /// accepted `dog = 5` and `dog = ["a"]` as happily as a table. Not reading
    /// what a dog wrote is the point; not caring whether it wrote a table at
    /// all is a different thing, and it would have made the one key this file
    /// adds the one key where a typo does not fail loudly. A defaulted map
    /// rather than `Option`, for the same reason: `None` and `null` would
    /// otherwise deserialize identically, and `null` is no more a table than
    /// `5` or `["a"]` is. `deserialize_with` is the custom function below,
    /// needed because the derived map deserializer lets `null` back through
    /// for one YAML backend; see that function's own comment.
    #[serde(default, deserialize_with = "deserialize_dog_table")]
    #[cfg_attr(
        feature = "schema",
        schemars(with = "BTreeMap<String, serde_json::Value>")
    )]
    pub(super) dog: BTreeMap<String, serde::de::IgnoredAny>,
    #[serde(default, rename = "app")]
    pub(super) apps: Vec<AppConfig>,
}

// `deserialize_any`, not the derived `BTreeMap` deserializer: at least
// one YAML backend folds a null scalar into an empty map before
// `deserialize_map` ever reaches a visitor. `deserialize_any` reaches
// that null as `visit_unit`, which errors by default.
fn deserialize_dog_table<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, serde::de::IgnoredAny>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DogTableVisitor;

    impl<'de> serde::de::Visitor<'de> for DogTableVisitor {
        type Value = BTreeMap<String, serde::de::IgnoredAny>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a table")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut out = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                out.insert(key, value);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(DogTableVisitor)
}

impl RawFlockfile {
    /// Parses `source`, refusing any key no field claims.
    ///
    /// Shared by `Flockfile::parse` and `parse_declared`: both read a
    /// Flockfile off disk (never a value off the wire), so both refuse a typo
    /// the same way. `deny_unknown_fields` used to live on `AppConfig`
    /// itself, which made every new Flockfile field a protocol event: the
    /// same type rides the wire, where an unknown field means a newer peer
    /// rather than a typo. The denial belongs here instead.
    pub(super) fn new(source: &str, format: FlockFormat) -> Result<Self, FlockfileError> {
        let mut unknown = Vec::new();
        let raw: RawFlockfile = parse_into_ignoring(source, format, |path| {
            // `dog` is a map of `IgnoredAny` by design (see its doc comment):
            // shep does not read or validate what a dog wrote there, so
            // serde_ignored's callback for a key inside it is not a typo, it
            // is the field doing exactly what it is for.
            if !path.starts_with("dog.") {
                unknown.push(path.to_string());
            }
        })?;
        if !unknown.is_empty() {
            return Err(FlockfileError::UnknownKeys { keys: unknown });
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::flockfile::file::Flockfile;
    use crate::config::flockfile::format::FlockFormat;

    /// fails if a Flockfile carrying a dog's own configuration is refused.
    ///
    /// A dog with per-app configuration has nowhere else to put it: it
    /// belongs beside the app's declaration, in the repository the dog
    /// deploys.
    ///
    /// The contents are not validated: shep does not know what a dog's
    /// keys mean, and refusing a document for not recognising another
    /// program's config is a coupling neither side wants.
    #[test]
    fn a_dog_table_is_accepted_and_ignored() {
        let src = r#"
[dog.deploy]
command = "npm run build"
artifacts = ["dist/app.js"]

[dog.some-other-dog]
anything = { nested = true, count = 3 }

[[app]]
name = "web"
script = "./srv"
"#;
        let flock =
            Flockfile::parse(src, FlockFormat::Toml).expect("a dog's table is not an error");
        assert_eq!(flock.apps.len(), 1);
        assert_eq!(flock.apps[0].name, "web");
    }

    /// fails if `dog` accepts something that is not a table.
    ///
    /// Not reading what a dog wrote does not mean not caring whether it
    /// wrote a table: that would make this the one key where a typo does
    /// not fail loudly.
    #[test]
    fn a_dog_that_is_not_a_table_is_refused() {
        for value in ["5", "\"nope\"", "[1, 2]", "true"] {
            let src = format!("dog = {value}\n\n[[app]]\nname = \"web\"\nscript = \"./srv\"\n");
            assert!(
                Flockfile::parse(&src, FlockFormat::Toml).is_err(),
                "`dog = {value}` is not a table and must be refused"
            );
        }
    }

    /// fails if `dog: null` is accepted in JSON. TOML has no null literal,
    /// so only JSON, YAML and JSON5 need this case, one test each.
    #[test]
    fn a_dog_that_is_null_is_refused_in_json() {
        let json = r#"{ "dog": null, "app": [{ "name": "web", "script": "./srv" }] }"#;
        assert!(
            Flockfile::parse(json, FlockFormat::Json).is_err(),
            "`dog: null` is not a table and must be refused in JSON"
        );
    }

    /// fails if `dog: null` is accepted in YAML. A YAML backend may treat a
    /// null scalar requested as a map as an empty map rather than a type
    /// error; `dog`'s own `deserialize_with` exists to close exactly that.
    #[test]
    fn a_dog_that_is_null_is_refused_in_yaml() {
        let yaml = "dog: null\napp:\n  - name: web\n    script: ./srv\n";
        assert!(
            Flockfile::parse(yaml, FlockFormat::Yaml).is_err(),
            "`dog: null` is not a table and must be refused in YAML"
        );
    }

    /// fails if `dog: null` is accepted in JSON5. JSON5 is parsed by its own
    /// crate, so this is not redundant with the JSON case above.
    #[test]
    fn a_dog_that_is_null_is_refused_in_json5() {
        let json5 = "{ dog: null, app: [{ name: 'web', script: './srv' }] }";
        assert!(
            Flockfile::parse(json5, FlockFormat::Json5).is_err(),
            "`dog: null` is not a table and must be refused in JSON5"
        );
    }

    /// fails if a typo anywhere else stops failing loudly.
    ///
    /// Exactly one more key is legal, which is the whole reason the table is
    /// nested under one name rather than allowing loose top-level keys.
    #[test]
    fn a_key_that_is_not_dog_still_fails() {
        let src = r#"
[build]
command = "npm run build"

[[app]]
name = "web"
script = "./srv"
"#;
        let err = Flockfile::parse(src, FlockFormat::Toml)
            .expect_err("an unknown top-level key must still be refused");
        assert!(
            format!("{err}").contains("build"),
            "the refusal must name the key: {err}"
        );
    }

    #[test]
    fn a_schema_key_is_accepted_and_ignored() {
        let src = r#"{ "$schema": "./flockfile.schema.json",
                       "app": [{ "name": "web", "script": "./srv" }] }"#;
        let flock = Flockfile::parse(src, FlockFormat::Json).unwrap();
        assert_eq!(flock.apps.len(), 1);
    }

    /// fails if the new field is implemented by relaxing
    /// `deny_unknown_fields` instead of naming one more key, which would
    /// silently accept every typo the document lock exists to catch.
    #[test]
    fn one_more_key_is_legal_and_no_others_are() {
        let src = r#"{ "schema": "x", "app": [{ "name": "w", "script": "./s" }] }"#;
        assert!(
            matches!(
                Flockfile::parse(src, FlockFormat::Json),
                Err(FlockfileError::Json(_))
            ),
            "bare `schema` (no $) must still be an unknown field"
        );
    }

    #[test]
    fn a_toml_flockfile_takes_the_key_too() {
        let src = "\"$schema\" = \"./flockfile.schema.json\"\n\
                   [[app]]\nname = \"web\"\nscript = \"./srv\"\n";
        assert_eq!(
            Flockfile::parse(src, FlockFormat::Toml).unwrap().apps.len(),
            1
        );
    }
}
