//! What each sheep named its lambs, and when a name stops applying
//!
//! Keyed by the sheep's root pid, the number a `Describe` row carries, so a
//! respawn starts with no labels. The polling tick drops a label once its
//! pid has left the root's tree. A label set after that tick's reading
//! began is kept: the reading may predate the lamb.
//!
//! A pid recycled inside the same tree between two ticks inherits the dead
//! lamb's label until the next one.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use shep_core::protocol::LambLabel;
use tokio::time::Instant;

use super::sample::TreeIndex;

/// The most labels one sheep process holds. Past it, the oldest goes.
// Far past any lamb table an operator reads; it bounds an app that labels
// every short-lived job faster than one tick prunes them.
pub(crate) const MAX_LABELS_PER_SHEEP: usize = 256;

/// One label, when it was set, and its place in setting order.
#[derive(Debug, Clone)]
struct Stamped {
    label: String,
    set_at: Instant,
    /// Breaks the tie a paused clock makes of `set_at`, so eviction always
    /// takes the label set first.
    seq: u64,
}

/// Every sheep's labels, by root pid and then by lamb pid.
#[derive(Debug, Default)]
struct Book {
    by_root: HashMap<u32, HashMap<u32, Stamped>>,
    next_seq: u64,
}

/// The labels sheep sent on their shepherd channels.
///
/// Each critical section is map work. The tree walks [`Self::prune`] needs
/// run outside the lock, so the actor setting a label never waits on one.
#[derive(Debug, Default)]
pub(crate) struct LambLabels {
    book: Mutex<Book>,
}

impl LambLabels {
    /// Records `label` for lamb `pid` of the sheep at `root_pid`. An empty
    /// label clears the one `pid` had.
    pub(crate) fn set(&self, root_pid: u32, pid: u32, label: &LambLabel, now: Instant) {
        let mut book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
        if label.is_empty() {
            if let Some(labels) = book.by_root.get_mut(&root_pid) {
                labels.remove(&pid);
                if labels.is_empty() {
                    book.by_root.remove(&root_pid);
                }
            }
            return;
        }
        let seq = book.next_seq;
        book.next_seq += 1;
        let labels = book.by_root.entry(root_pid).or_default();
        labels.insert(
            pid,
            Stamped {
                label: label.as_str().to_owned(),
                set_at: now,
                seq,
            },
        );
        if labels.len() > MAX_LABELS_PER_SHEEP {
            let oldest = labels
                .iter()
                .min_by_key(|(_, stamped)| stamped.seq)
                .map(|(&pid, _)| pid);
            if let Some(oldest) = oldest {
                labels.remove(&oldest);
            }
        }
    }

    /// The labels the sheep at `root_pid` holds, by lamb pid.
    pub(crate) fn of(&self, root_pid: u32) -> HashMap<u32, String> {
        let book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
        book.by_root
            .get(&root_pid)
            .map(|labels| {
                labels
                    .iter()
                    .map(|(&pid, stamped)| (pid, stamped.label.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Drops each label set before `taken_at` whose pid `index` no longer
    /// places under its root. A root gone from `index` loses all of those.
    ///
    /// `taken_at` must be read before the table `index` was built from.
    pub(crate) fn prune(&self, index: &TreeIndex, taken_at: Instant) {
        let roots: Vec<u32> = {
            let book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
            book.by_root.keys().copied().collect()
        };
        let trees: HashMap<u32, HashSet<u32>> = roots
            .into_iter()
            .map(|root| (root, index.descendants_of(root)))
            .collect();
        let mut book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
        book.by_root.retain(|root, labels| {
            // A root first labelled since the walks above has no tree here,
            // and every label it holds is newer than `taken_at`.
            let tree = trees.get(root);
            labels.retain(|pid, stamped| {
                stamped.set_at >= taken_at || tree.is_some_and(|tree| tree.contains(pid))
            });
            !labels.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::testing::rss;

    fn label(text: &str) -> LambLabel {
        LambLabel::new(text).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn a_label_is_kept_per_root_and_replaced_by_a_second() {
        let labels = LambLabels::default();
        let now = Instant::now();
        labels.set(100, 101, &label("worker 1"), now);
        labels.set(100, 101, &label("worker 9"), now);
        labels.set(200, 101, &label("another sheep's"), now);

        assert_eq!(
            labels.of(100),
            HashMap::from([(101, "worker 9".to_string())])
        );
        assert_eq!(
            labels.of(200),
            HashMap::from([(101, "another sheep's".to_string())])
        );
        assert!(labels.of(300).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_label_clears_the_one_it_names() {
        let labels = LambLabels::default();
        let now = Instant::now();
        labels.set(100, 101, &label("worker 1"), now);
        labels.set(100, 102, &label("worker 2"), now);

        labels.set(100, 101, &label(""), now);

        assert_eq!(
            labels.of(100),
            HashMap::from([(102, "worker 2".to_string())])
        );
    }

    #[tokio::test(start_paused = true)]
    async fn past_the_cap_the_label_set_first_goes() {
        let labels = LambLabels::default();
        let now = Instant::now();
        let first_pid = 1000;
        let over = u32::try_from(MAX_LABELS_PER_SHEEP).unwrap() + 1;
        for pid in first_pid..first_pid + over {
            labels.set(1, pid, &label("job"), now);
        }

        let held = labels.of(1);
        assert_eq!(held.len(), MAX_LABELS_PER_SHEEP);
        assert!(!held.contains_key(&first_pid));
        assert!(held.contains_key(&(first_pid + over - 1)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_tick_drops_a_pid_that_left_the_tree_and_keeps_one_still_in_it() {
        let labels = LambLabels::default();
        labels.set(100, 101, &label("stays"), Instant::now());
        labels.set(100, 102, &label("exited"), Instant::now());
        tokio::time::advance(Duration::from_secs(1)).await;

        let taken_at = Instant::now();
        let index = TreeIndex::build(&[rss(100, None, 1), rss(101, Some(100), 1)]);
        labels.prune(&index, taken_at);

        assert_eq!(labels.of(100), HashMap::from([(101, "stays".to_string())]));
    }

    /// The race the stamp exists for: a lamb spawned and labelled after the
    /// reading began is absent from it.
    #[tokio::test(start_paused = true)]
    async fn a_label_set_after_the_reading_began_survives_that_tick() {
        let labels = LambLabels::default();
        let taken_at = Instant::now();
        let index = TreeIndex::build(&[rss(100, None, 1)]);
        tokio::time::advance(Duration::from_millis(5)).await;
        labels.set(100, 101, &label("just spawned"), Instant::now());

        labels.prune(&index, taken_at);

        assert_eq!(
            labels.of(100),
            HashMap::from([(101, "just spawned".to_string())])
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_root_gone_from_the_table_loses_every_label() {
        let labels = LambLabels::default();
        labels.set(100, 101, &label("worker 1"), Instant::now());
        tokio::time::advance(Duration::from_secs(1)).await;

        let index = TreeIndex::build(&[rss(101, Some(1), 1)]);
        labels.prune(&index, Instant::now());

        assert!(labels.of(100).is_empty());
        assert!(labels.book.lock().unwrap().by_root.is_empty());
    }

    /// A pid in another sheep's tree is not this sheep's lamb.
    #[tokio::test(start_paused = true)]
    async fn a_label_on_a_pid_outside_the_tree_goes_at_the_next_tick() {
        let labels = LambLabels::default();
        labels.set(100, 201, &label("not mine"), Instant::now());
        tokio::time::advance(Duration::from_secs(1)).await;

        let index =
            TreeIndex::build(&[rss(100, None, 1), rss(200, None, 1), rss(201, Some(200), 1)]);
        labels.prune(&index, Instant::now());

        assert!(labels.of(100).is_empty());
    }
}
