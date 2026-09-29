//! A sheep's lamb tree as `describe`'s second table: [`LambRows`].

use serde::Serialize;
use shep_core::protocol::Lamb;

use crate::output::Render;

/// One sheep's lamb tree, as `describe`'s second table.
///
/// Not `#[serde(transparent)]`: this type's JSON is never read, since
/// `describe --format json` serializes the listing as
/// [`FlockRows`](super::FlockRows) with its own `lambs`. It exists to reach
/// [`render_table`](crate::output::render_table).
#[derive(Debug, Serialize)]
pub struct LambRows(pub Vec<Lamb>);

/// No colour: both columns are identity, and a lamb has no status, reading or
/// placeholder for one to carry.
impl Render for LambRows {
    fn headers() -> &'static [&'static str] {
        &["PID", "NAME"]
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.0
            .iter()
            .map(|lamb| vec![lamb.pid.to_string(), lamb.name.clone()])
            .collect()
    }

    /// # Panics
    /// If `header` is not one of `Self::headers()`'s own values.
    #[track_caller]
    fn json_key_for(header: &str) -> &'static str {
        match header {
            "PID" => "pid",
            "NAME" => "name",
            other => panic!("LambRows::headers() does not include {other:?}"),
        }
    }

    const JSON_ONLY: &'static [&'static str] = &[];

    // Parallel to `headers()`. Two columns, both identity, so this never
    // narrows; spelled out so a later header does not inherit it by omission.
    const PRIORITIES: &'static [u8] = &[0, 0];
}

#[cfg(test)]
mod tests {
    use super::super::tests::{assert_no_drift, coloured};
    use super::*;

    #[test]
    fn lamb_rows_do_not_drift() {
        assert_no_drift(
            &LambRows(vec![Lamb::new(4243, "node"), Lamb::new(4244, "sh")]),
            |j| &j[0],
            &[],
        );
    }

    #[test]
    fn lamb_rows_carry_no_colour_at_all() {
        let rows = LambRows(vec![Lamb::new(48_302, "node")]).rows_for(coloured(), true);
        assert_eq!(rows[0], vec!["48302".to_string(), "node".to_string()]);
    }
}
