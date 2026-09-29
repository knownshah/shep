//! Closing a pane whose write replaces a whole table.
//!
//! `Request::SetSheepDogSettings` and `Request::SetDogConfig` each replace
//! the table they name. A write built on what the pane read at open would
//! undo any change another writer made since. So `Escape` reads the table
//! again before writing: an unchanged table is written and the pane
//! closes, and a moved one stays on screen under the operator's edits.

use super::super::*;

impl App {
    /// `Escape` on a dog or per-sheep table pane holding edits: reads the
    /// table again, and holds the pane until the answer lands.
    ///
    /// [`None`] for every other pane and for one with nothing filed, which
    /// close at once. A second `Escape` while the read is out asks nothing.
    pub(in crate::lookout::app) fn reread_before_closing(&mut self) -> Option<Effect> {
        let pane = self.config_pane()?;
        let whole_table = matches!(pane.target(), PaneTarget::SheepDog { .. });
        if !whole_table || pane.edits().is_empty() {
            return None;
        }
        if self.closing {
            return Some(Effect::None);
        }
        self.closing = true;
        Some(self.reread_pane())
    }

    /// The answer to [`Self::reread_before_closing`]'s read.
    ///
    /// The table the pane holds: the write goes out and the pane closes. A
    /// moved table: the pane takes it under the same edits, and writes
    /// nothing until the next `Escape`. A failed read changes nothing.
    pub(in crate::lookout::app) fn on_close_reread(
        &mut self,
        name: &str,
        result: Result<Response, RequestError>,
    ) -> Effect {
        self.closing = false;
        let Some(pane) = self.config_pane() else {
            return Effect::None;
        };
        let why = match (result, pane.target().clone()) {
            (Ok(Response::SheepConfig(view)), PaneTarget::SheepDog { sheep, dog }) => {
                let fresh = view.config.dogs.get(&dog).map(DogTable::as_map);
                if pane.holds_table(fresh) {
                    return self.write_and_close();
                }
                self.refresh_sheep_dog_pane(&view);
                return self.hold(format!("{sheep}: its {dog} table"));
            }
            (Ok(_unrecognised), _) => {
                "the shepherd answered something this lookout does not understand".to_owned()
            }
            (Err(RequestError::Rpc(err)), _) => err.message,
            (Err(other), _) => other.to_string(),
        };
        self.notice = Some(Notice {
            text: format!("{name}: {why}, so nothing was written"),
            grave: true,
        });
        Effect::None
    }

    /// Ends a close's wait on a read the link task never took, and returns
    /// what the notice about that read adds.
    pub(in crate::lookout::app) fn end_close(&mut self) -> &'static str {
        if core::mem::take(&mut self.closing) {
            ", so nothing was written"
        } else {
            ""
        }
    }

    /// Sends everything the pane has filed and closes it.
    fn write_and_close(&mut self) -> Effect {
        let writes = self.take_pane_writes();
        self.close_pane();
        if writes.is_empty() {
            Effect::None
        } else {
            Effect::SendAll(writes)
        }
    }

    /// Says why a pane that took a moved table is still up. `subject`
    /// names the table and never a value in it.
    fn hold(&mut self, subject: String) -> Effect {
        self.notice = Some(Notice {
            text: format!(
                "{subject} changed while this pane was open, so nothing was written; \
                 esc writes your edits over the new values"
            ),
            grave: true,
        });
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use shep_core::protocol::{RpcError, RpcErrorCode};

    use super::super::super::*;
    use crate::lookout::app::testing::*;

    /// What a second writer rotates `jobs`'s token to while the pane is
    /// open. No more a notice's to print than [`TOKEN`] is.
    const ROTATED: &str = "sk-live-rotated-77Zq";

    /// `web_view(true)` with `jobs` replaced by `table`.
    fn web_with_jobs(table: &serde_json::Value) -> SheepConfigView {
        let mut view = web_view(true);
        view.config.dogs.insert(
            "jobs".into(),
            table.as_object().cloned().expect("a table").into(),
        );
        view
    }

    /// `web`'s config read, answered with `view`.
    fn answer_web(app: &mut App, view: SheepConfigView) -> Effect {
        app.update(Msg::Replied {
            sent: Sent::SheepConfig { name: "web".into() },
            result: Ok(Response::SheepConfig(Box::new(view))),
        })
    }

    /// `app_in_jobs_table` with `concurrency` edited to 4 and `Escape`
    /// pressed, so the re-read is out. `close_jobs_table` answers it.
    fn closing_jobs() -> App {
        let mut app = app_in_jobs_table();
        type_concurrency(&mut app, "4");
        assert_eq!(
            app.update(Msg::Key(KeyPress::Escape)),
            Effect::Send(Sent::SheepConfig { name: "web".into() }),
            "esc reads the sheep again before writing"
        );
        app
    }

    /// The one table write a batch carries, as JSON.
    fn table_written(effect: Effect) -> serde_json::Value {
        let batch = wire_batch(effect);
        let [
            Sent::SetSheepDogTable {
                name,
                dog,
                table: Some(table),
                ..
            },
        ] = batch.as_slice()
        else {
            panic!("one table write: {batch:?}");
        };
        assert_eq!((name.as_str(), dog.as_str()), ("web", "jobs"));
        serde_json::Value::Object(table.as_map().clone())
    }

    #[test]
    fn esc_on_an_edited_table_pane_holds_it_until_the_re_read_answers() {
        let app = closing_jobs();
        let pane = app.config_pane().expect("the pane waits on the answer");
        assert!(
            matches!(pane.target(), PaneTarget::SheepDog { .. }),
            "{pane:?}"
        );
        assert_eq!(pane.edits().len(), 1, "nothing has left the pane yet");
    }

    /// A second writer rotated `token` and added `retries` while the pane
    /// was open. Both survive the operator's own write, which the next
    /// `Escape` sends.
    #[test]
    fn a_moved_table_holds_the_pane_and_the_next_close_keeps_both_writes() {
        let mut app = closing_jobs();
        let moved = json!({ "concurrency": 2, "token": ROTATED, "retries": 5 });
        assert_eq!(answer_web(&mut app, web_with_jobs(&moved)), Effect::None);
        let pane = app.config_pane().expect("the pane stays up");
        assert!(
            matches!(pane.target(), PaneTarget::SheepDog { .. }),
            "{pane:?}"
        );
        assert_eq!(pane.edits().len(), 1, "the edit rides the new table");
        let notice = app.notice().expect("the hold is said");
        assert!(notice.is_grave());
        assert_eq!(
            notice.to_string(),
            "web: its jobs table changed while this pane was open, so nothing was written; \
             esc writes your edits over the new values"
        );

        assert_eq!(
            app.update(Msg::Key(KeyPress::Escape)),
            Effect::Send(Sent::SheepConfig { name: "web".into() })
        );
        let written = table_written(answer_web(&mut app, web_with_jobs(&moved)));
        assert_eq!(
            written,
            json!({ "concurrency": 4, "token": ROTATED, "retries": 5 })
        );
    }

    /// A second writer removed the table: writing the pane's copy back
    /// would restore its `token` as well as the edit.
    #[test]
    fn a_table_removed_while_the_pane_was_open_holds_it() {
        let mut app = closing_jobs();
        assert_eq!(answer_web(&mut app, web_view(false)), Effect::None);
        assert_eq!(app.config_pane().expect("still up").edits().len(), 1);
        let notice = app.notice().expect("the hold is said").to_string();
        assert!(notice.contains("nothing was written"), "{notice}");

        let _ = app.update(Msg::Key(KeyPress::Escape));
        let written = table_written(answer_web(&mut app, web_view(false)));
        assert_eq!(written, json!({ "concurrency": 4 }));
    }

    #[test]
    fn a_failed_re_read_holds_the_pane_and_writes_nothing() {
        let mut app = closing_jobs();
        let effect = app.update(Msg::Replied {
            sent: Sent::SheepConfig { name: "web".into() },
            result: Err(RequestError::Rpc(RpcError {
                code: RpcErrorCode::NotFound,
                message: "no sheep named web".into(),
                daemon_version: None,
            })),
        });
        assert_eq!(effect, Effect::None);
        assert_eq!(app.config_pane().expect("still up").edits().len(), 1);
        let notice = app.notice().expect("reported");
        assert!(notice.is_grave());
        assert_eq!(
            notice.to_string(),
            "web: no sheep named web, so nothing was written"
        );
    }

    /// The wait ends with the read the link task never took, so a later
    /// `r` is a refresh and not a close.
    #[test]
    fn an_unsent_re_read_ends_the_wait_and_a_later_r_only_refreshes() {
        let mut app = closing_jobs();
        let _ = app.update(Msg::Unsent {
            sent: Sent::SheepConfig { name: "web".into() },
        });
        assert_eq!(
            app.notice().map(ToString::to_string).as_deref(),
            Some("web: its config was not asked for, so nothing was written")
        );
        assert_eq!(
            app.update(Msg::Key(KeyPress::Refresh)),
            Effect::Send(Sent::SheepConfig { name: "web".into() })
        );
        assert_eq!(answer_web(&mut app, web_view(true)), Effect::None);
        assert_eq!(app.config_pane().expect("still up").edits().len(), 1);
    }

    #[test]
    fn a_second_esc_while_the_re_read_is_out_asks_nothing_more() {
        let mut app = closing_jobs();
        assert_eq!(app.update(Msg::Key(KeyPress::Escape)), Effect::None);
        let written = table_written(answer_web(&mut app, web_view(true)));
        assert_eq!(written, json!({ "concurrency": 4, "token": TOKEN }));
    }
}
