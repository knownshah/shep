//! A dog's per-sheep tables: the read and the write.
//!
//! Every answer comes off one view of the flock, [`Actor::dog_tables`], so
//! the read and anything that compares it later agree about which slot
//! speaks for a name. The write is [`Actor::write_sheep_field`] on the whole
//! `dogs` map, built here from the one table that moved.

use super::*;

/// Sheep name to that sheep's `dogs` map, borrowed from the flock.
type DogView<'a> = BTreeMap<&'a str, &'a BTreeMap<String, DogTable>>;

impl<R: ProcessRunner> Actor<R> {
    /// Every non-dog name carrying a table, read off the stored spec of the
    /// slot [`Self::representative_id`] would pick.
    ///
    /// One pass rather than `representative_id` per name, which would walk
    /// the flock once for every sheep in it.
    fn dog_tables(&self) -> DogView<'_> {
        // Ranked as `representative_id` ranks: a non-drainee first, then by
        // instance, then by id.
        let mut chosen: BTreeMap<&str, (bool, u32, u32)> = BTreeMap::new();
        for (id, slot) in &self.sheep {
            if slot.entry.dog.is_some() {
                continue;
            }
            let draining = matches!(slot.entry.reload, ReloadState::Drainee { .. });
            let rank = (draining, slot.entry.instance, *id);
            chosen
                .entry(slot.entry.spec.config().name.as_str())
                .and_modify(|held| *held = rank.min(*held))
                .or_insert(rank);
        }
        chosen
            .into_iter()
            .filter_map(|(name, (_, _, id))| {
                let dogs = &self.sheep.get(&id)?.entry.spec.config().dogs;
                (!dogs.is_empty()).then_some((name, dogs))
            })
            .collect()
    }

    /// `dog`'s table on every sheep carrying one, by sheep name.
    ///
    /// Empty when no sheep carries one, never a refusal: a dog asks on its
    /// own name, which cannot be missing the way a sheep's can.
    pub(super) fn handle_dog_sheep_settings(&self, dog: &str) -> BTreeMap<String, DogTable> {
        self.dog_tables()
            .into_iter()
            .filter_map(|(name, dogs)| Some((name.to_string(), dogs.get(dog)?.clone())))
            .collect()
    }

    /// Sets `dog`'s table on `name`, or removes it with `None`, as an
    /// operator override of the whole `dogs` field.
    ///
    /// `Ok(None)` when no sheep has that name. The map is built from the
    /// intended config, so every other dog's table rides along as it is,
    /// and a parked edit is not dropped for `apply_one`'s reason.
    ///
    /// # Errors
    ///
    /// Whatever [`Self::write_sheep_field`] refuses: `IsADog` for a dog's
    /// own name, `InvalidField` for an empty dog name, `Overrides` for a
    /// store that could not be read or written.
    pub(super) fn handle_set_sheep_dog_settings(
        &mut self,
        name: &str,
        dog: &str,
        table: Option<DogTable>,
    ) -> Result<Option<FieldSet>, SupervisorError> {
        let Some(intended) = self
            .representative_id(name)
            .and_then(|id| self.intended_spec(id))
        else {
            return Ok(None);
        };
        let mut dogs = intended.config().dogs.clone();
        match table {
            Some(table) => dogs.insert(dog.to_string(), table),
            None => dogs.remove(dog),
        };
        // No serde message in the refusal: it can quote a value.
        let value = serde_json::to_value(&dogs).map_err(|_| {
            SupervisorError::InvalidField(format!("dogs: {dog}'s table would not serialize"))
        })?;
        self.write_sheep_field(name, "dogs", &value)
    }
}
