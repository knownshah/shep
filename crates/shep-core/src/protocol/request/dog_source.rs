//! Where a dog came from, as a flock row carries it.

use serde::{Deserialize, Serialize};

// Named by an intra-doc link and by nothing rustc compiles.
#[cfg(doc)]
use super::ProcessInfo;

/// Where a dog came from: this binary, or one an operator adopted.
///
/// Carried on [`ProcessInfo::dog`], so a listing distinguishes the two
/// populations without a second request.
// wire format: changing existing variants is a breaking change
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DogSource {
    /// An argv branch of the shep binary itself (`shep dog <name>`).
    BuiltIn,
    /// A binary an operator adopted, run at the daemon's own trust level.
    Adopted {
        /// The binary's path, exactly as the operator gave it to `adopt`.
        path: String,
    },
}

impl DogSource {
    /// The kind's one-word label, `built-in` or `adopted`.
    ///
    /// Never the adopted path: this is what a SOURCE column shows, and the
    /// path is on the variant for a caller that wants it.
    const fn as_str(&self) -> &'static str {
        match self {
            DogSource::BuiltIn => "built-in",
            DogSource::Adopted { .. } => "adopted",
        }
    }
}

/// The kind's label, for a caller rendering a source into a cell or a
/// metric value. Takes a borrow, since [`DogSource::Adopted`] owns a
/// `String` that the label does not read.
impl From<&DogSource> for &'static str {
    fn from(source: &DogSource) -> Self {
        source.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dog_source_serializes_snake_case_under_its_kind() {
        assert_eq!(
            serde_json::to_string(&DogSource::BuiltIn).unwrap(),
            r#"{"kind":"built_in"}"#
        );
        let adopted = DogSource::Adopted {
            path: "/usr/local/bin/shep-otel".to_string(),
        };
        let wire = r#"{"kind":"adopted","path":"/usr/local/bin/shep-otel"}"#;
        assert_eq!(serde_json::to_string(&adopted).unwrap(), wire);
        assert_eq!(serde_json::from_str::<DogSource>(wire).unwrap(), adopted);
    }
}
