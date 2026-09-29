//! The detail pane's lamb line: the selected sheep's last lamb walk.
//!
//! Fetched with a `Request::Describe` on selection change and on `r`, never
//! on the poll, so it carries its own age.

use ratatui::text::{Line, Span};

use super::super::app::{App, LambWalk};
use super::flock::fit;
use crate::output::human_duration;

/// The lamb line: what the last walk found, and how old it is.
///
/// The age comes first: a truncated list is still honest, but a list whose
/// stamp truncated away is a stale reading presented as current.
///
/// Omits the CLI's "not exactly the set a stop kills" caveat, since
/// "parent-pid descendants" is already precise.
pub(super) fn lamb_line(
    app: &App,
    id: u32,
    width: u16,
    palette: super::super::theme::Palette,
) -> Line<'static> {
    let text = match app.lambs_for(id) {
        None => "lambs  not read yet".to_string(),
        Some((LambWalk::Failed, _)) => {
            "lambs  the shepherd did not answer that request".to_string()
        }
        Some((LambWalk::NotWalked, _)) => {
            "lambs  this sheep is not running, so there is no tree to walk".to_string()
        }
        Some((LambWalk::Walked(lambs), age)) if lambs.is_empty() => {
            format!("lambs  none found, read {} ago", human_duration(age))
        }
        Some((LambWalk::Walked(lambs), age)) => {
            let noun = if lambs.len() == 1 {
                "descendant"
            } else {
                "descendants"
            };
            let list = lambs
                .iter()
                .map(|lamb| format!("{} {}", lamb.pid, lamb.name))
                .collect::<Vec<_>>()
                .join("   ");
            format!(
                "lambs  {} parent-pid {noun}, read {} ago   {list}",
                lambs.len(),
                human_duration(age)
            )
        }
    };
    Line::from(Span::styled(fit(&text, width), palette.muted()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use shep_core::protocol::Lamb;

    use super::super::detail::detail_lines;
    use super::super::fixtures::{
        app_with_lamb_reading_at, lamb_line_of, render_all, rendered, sheep_with_lambs,
        with_lamb_reading, with_lamb_reading_for, with_selection,
    };
    use crate::lookout::app::{LambWalk, Msg};

    /// Five states, and the CLI's own wording covers only one of them: the
    /// other four sentences belong to this pane.
    #[test]
    fn the_pane_says_which_lamb_state_it_is_in() {
        let cases: [(LambWalk, &str); 3] = [
            (
                LambWalk::Walked(vec![Lamb::new(48_220, "node"), Lamb::new(48_221, "node")]),
                "lambs  2 parent-pid descendants, read ",
            ),
            (LambWalk::Walked(Vec::new()), "lambs  none found, read "),
            (
                LambWalk::NotWalked,
                "lambs  this sheep is not running, so there is no tree to walk",
            ),
        ];
        for (walk, expected) in cases {
            let app = with_lamb_reading(walk);
            let rendered = render_all(&detail_lines(&app, 200));
            assert!(
                rendered.contains(expected),
                "expected {expected:?} in {rendered:?}"
            );
        }

        let failed = with_lamb_reading(LambWalk::Failed);
        assert!(
            render_all(&detail_lines(&failed, 200))
                .contains("lambs  the shepherd did not answer that request")
        );

        let unread = with_selection(sheep_with_lambs());
        assert!(render_all(&detail_lines(&unread, 200)).contains("lambs  not read yet"));
    }

    #[test]
    fn one_lamb_is_a_descendant_and_not_descendants() {
        let app = with_lamb_reading(LambWalk::Walked(vec![Lamb::new(48_220, "node")]));
        let rendered = render_all(&detail_lines(&app, 200));
        assert!(
            rendered.contains("1 parent-pid descendant, read "),
            "got {rendered:?}"
        );
    }

    #[test]
    fn the_lamb_line_carries_its_age_before_its_list() {
        let app = with_lamb_reading(LambWalk::Walked(vec![Lamb::new(48_220, "node")]));
        let line = rendered(&detail_lines(&app, 200)[1]);
        let stamp = line.find("read ").expect("a stamp");
        let list = line.find("48220").expect("a list");
        assert!(stamp < list, "the caveat must survive truncation: {line:?}");
    }

    #[test]
    fn a_reading_for_another_sheep_is_not_drawn_here() {
        // with_lamb_reading pins its reading to the selected sheep's id;
        // this one pins it to a different one and expects the unread sentence.
        let app = with_lamb_reading_for(11, LambWalk::Walked(vec![Lamb::new(48_220, "node")]));
        assert!(render_all(&detail_lines(&app, 200)).contains("lambs  not read yet"));
    }

    /// A two-age frame comparison can't fail here, since both renders share
    /// one wall-clock instant. Ages come from `Msg::Tick` arithmetic
    /// instead.
    #[test]
    fn the_stamp_ages_on_a_live_dashboard_and_stops_on_a_frozen_one() {
        let (mut app, t0) =
            app_with_lamb_reading_at(LambWalk::Walked(vec![Lamb::new(48_220, "node")]));
        app.update(Msg::Tick {
            now: t0 + Duration::from_secs(120),
        });
        let live = lamb_line_of(&app);
        assert!(live.contains("read 2m ago"), "the stamp aged: {live:?}");

        app.update(Msg::Frozen {
            at_local: "2026-08-16 09:00:00".to_string(),
            why: super::super::fixtures::FROZEN_WHY.to_string(),
        });
        app.update(Msg::Tick {
            now: t0 + Duration::from_secs(3_600),
        });
        assert_eq!(
            lamb_line_of(&app),
            live,
            "a frozen dashboard's reading must not age"
        );
    }
}
