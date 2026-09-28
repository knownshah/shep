//! The dogs sub-screen: one row per dog that can hold a table on this
//! sheep, what state its table is in, and the table's key names.
//!
//! Never a value. A set table's secrets are marked by a schema the row does
//! not read, and a read-only table has no schema at all, so a row names
//! keys and stops there; the dog's own pane is where values are drawn,
//! masked.

use ratatui::text::{Line, Span};

use super::super::super::pane::{ConfigPane, DogTableState, DogsPane};
use super::super::super::theme::Palette;
use super::super::flock::{fit, mark};
use super::super::scroll::{self, Attempt};
use super::layout::body_width;

/// The widest a dog's name column grows before it is cut.
const NAME_W: u16 = 16;
/// `read-only`, the widest state word.
const STATE_W: u16 = 9;

/// The sub-screen: a title naming the sheep, or the armed removal's
/// question in its place, then the rows, laid out through
/// [`scroll::to_cursor`] so the cursor is drawn at every height.
pub(super) fn dogs_lines(
    pane: &ConfigPane,
    dogs: &DogsPane,
    palette: Palette,
    width: u16,
    budget: usize,
) -> Vec<Line<'static>> {
    let sheep = pane.target().name();
    let title = match dogs.armed() {
        Some(dog) => Line::from(Span::styled(
            format!(
                "  {}",
                fit(
                    &format!("remove {sheep}'s {dog} table? enter confirms, any other key cancels"),
                    body_width(width)
                )
            ),
            palette.attention(),
        )),
        None => Line::from(Span::styled(
            format!(
                "  {}",
                fit(&format!("{sheep} \u{203a} dogs"), body_width(width))
            ),
            palette.muted(),
        )),
    };
    let mut lines = vec![title];
    let body_budget = budget.saturating_sub(1);
    if body_budget == 0 {
        return lines;
    }
    if dogs.rows().is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                fit(
                    "no dog publishes a sheep schema, and this sheep carries no table",
                    body_width(width)
                )
            ),
            palette.muted(),
        )));
        return lines;
    }
    let cursor_row = dogs.view().cursor().min(dogs.rows().len() - 1);
    lines.extend(scroll::to_cursor(
        cursor_row,
        dogs.view().offset(),
        |offset| dogs_body_from(dogs, palette, width, body_budget, offset),
        || vec![dog_line(dogs, cursor_row, true, width, palette)],
    ));
    lines
}

/// One row: the selection mark, the dog's name, its table's state, and
/// what the table holds, by key name only.
fn dog_line(
    dogs: &DogsPane,
    index: usize,
    selected: bool,
    width: u16,
    palette: Palette,
) -> Line<'static> {
    let Some(row) = dogs.rows().get(index) else {
        return Line::default();
    };
    let body = body_width(width);
    let name_w = NAME_W.min(body);
    let state_w = STATE_W.min(body.saturating_sub(name_w + 2));
    let rest_w = body.saturating_sub(name_w + state_w + 4);
    let (state, rest, muted) = match row.state() {
        DogTableState::Set => ("set", row.keys().join(", "), false),
        DogTableState::Unset => ("-", "(no table)".to_owned(), true),
        DogTableState::ReadOnly => (
            "read-only",
            format!("{}  (publishes no sheep schema)", row.keys().join(", ")),
            true,
        ),
    };
    let mut text = format!("{} ", mark(selected));
    text.push_str(&fit(row.name(), name_w));
    if state_w > 0 {
        text.push_str("  ");
        text.push_str(&fit(state, state_w));
    }
    if rest_w > 0 {
        text.push_str("  ");
        text.push_str(&fit(&rest, rest_w));
    }
    if muted {
        Line::from(Span::styled(text, palette.muted()))
    } else {
        Line::from(Span::raw(text))
    }
}

/// Lays the rows out from `offset`, spending at most `budget` lines, both
/// markers reserved before a row is admitted, as the list sub-screen does.
fn dogs_body_from(
    dogs: &DogsPane,
    palette: Palette,
    width: u16,
    budget: usize,
    offset: usize,
) -> Attempt {
    let total = dogs.rows().len();
    let cursor_row = dogs.view().cursor().min(total.saturating_sub(1));
    let above = usize::from(offset > 0);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut drawn = 0usize;
    for index in offset..total {
        if lines.len() + 1 + above + usize::from(index + 1 < total) > budget {
            break;
        }
        lines.push(dog_line(dogs, index, index == cursor_row, width, palette));
        drawn += 1;
    }
    let hidden_below = total.saturating_sub(offset + drawn);
    if hidden_below > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ... {hidden_below} below"),
            palette.muted(),
        )));
    }
    if offset > 0 {
        lines.insert(
            0,
            Line::from(Span::styled(
                format!("  ... {offset} above"),
                palette.muted(),
            )),
        );
    }
    Attempt {
        cursor_drawn: drawn > 0 && (offset..offset + drawn).contains(&cursor_row),
        lines,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use shep_core::config::AppConfig;
    use shep_core::protocol::SheepConfigView;

    use super::super::pane_lines;
    use super::*;
    use crate::lookout::pane::SheepDogEntry;
    use crate::lookout::view::fixtures::{plain, render_all};

    const TOKEN: &str = "sk-live-51Hx9Qa";

    /// `web` with a `jobs` table holding a credential and a `legacy` table
    /// no schema describes, and the sub-screen open over them.
    fn web_with_dogs_open() -> ConfigPane {
        let mut config = AppConfig {
            name: "web".into(),
            ..AppConfig::default()
        };
        let table = |value: serde_json::Value| value.as_object().cloned().expect("a table");
        config.dogs.insert(
            "jobs".into(),
            table(json!({ "concurrency": 2, "token": TOKEN })).into(),
        );
        config.dogs.insert(
            "legacy".into(),
            table(json!({ "url": format!("https://ops:{TOKEN}@example.test") })).into(),
        );
        let mut pane = ConfigPane::sheep(SheepConfigView::new(config, Vec::new(), Vec::new()));
        pane.open_dogs(vec![
            SheepDogEntry {
                name: "jobs".into(),
                adopted_path: None,
                schema: Some(json!({ "type": "object", "properties": {} })),
            },
            SheepDogEntry {
                name: "deploy".into(),
                adopted_path: None,
                schema: Some(json!({ "type": "object", "properties": {} })),
            },
        ]);
        pane
    }

    #[test]
    fn a_row_names_its_state_and_its_keys_and_never_a_value() {
        let text = render_all(&pane_lines(&web_with_dogs_open(), plain(), 120, 0));
        assert!(text.contains("web \u{203a} dogs"), "{text}");
        let row = |dog: &str| {
            text.lines()
                .find(|line| line.contains(dog))
                .unwrap_or_else(|| panic!("{dog} has no row: {text}"))
                .to_owned()
        };
        assert!(row("deploy").contains("(no table)"), "{text}");
        assert!(row("jobs").contains("set"), "{text}");
        assert!(row("jobs").contains("concurrency, token"), "{text}");
        assert!(row("legacy").contains("read-only"), "{text}");
        assert!(row("legacy").contains("url"), "{text}");
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains('2'), "no value, not even a number: {text}");
    }

    #[test]
    fn an_armed_removal_asks_in_the_title() {
        let mut pane = web_with_dogs_open();
        let dogs = pane.dogs_mut().expect("open");
        dogs.move_by(1);
        assert!(dogs.arm_removal());
        let text = render_all(&pane_lines(&pane, plain(), 120, 0));
        assert!(
            text.contains("remove web's jobs table? enter confirms"),
            "{text}"
        );
    }

    #[test]
    fn no_dog_and_no_table_says_so_under_the_title() {
        let mut pane = ConfigPane::sheep(SheepConfigView::new(
            AppConfig {
                name: "web".into(),
                ..AppConfig::default()
            },
            Vec::new(),
            Vec::new(),
        ));
        pane.open_dogs(Vec::new());
        let text = render_all(&pane_lines(&pane, plain(), 120, 0));
        assert_eq!(
            text.lines().map(str::trim).collect::<Vec<_>>(),
            [
                "web \u{203a} dogs",
                "no dog publishes a sheep schema, and this sheep carries no table",
            ],
            "{text}"
        );
    }

    #[test]
    fn the_cursor_is_drawn_at_a_height_that_cannot_hold_every_row() {
        let mut pane = web_with_dogs_open();
        pane.dogs_mut().expect("open").move_to_last();
        let text = render_all(&pane_lines(&pane, plain(), 120, 3));
        assert!(text.contains("> legacy"), "{text}");
        assert!(text.contains("... 2 above"), "{text}");
        assert_eq!(text.lines().count(), 3, "{text}");
    }
}
