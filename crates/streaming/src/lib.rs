pub mod scheduler;

//! Testable publication-generation invariant; GPU synchronization arrives in B3.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildToken {
    generation: u64,
    revision: u64,
}

impl BuildToken {
    pub fn revision(self) -> u64 {
        self.revision
    }
}

#[derive(Debug)]
pub struct Residency<T> {
    published: Option<(u64, T)>,
    building: Option<BuildToken>,
    next_generation: u64,
}

impl<T> Default for Residency<T> {
    fn default() -> Self {
        Self {
            published: None,
            building: None,
            next_generation: 0,
        }
    }
}

impl<T> Residency<T> {
    pub fn visible(&self) -> Option<&T> {
        self.published.as_ref().map(|(_, value)| value)
    }

    pub fn visible_revision(&self) -> Option<u64> {
        self.published.as_ref().map(|(revision, _)| *revision)
    }

    /// Starting a rebuild does not remove the current published representation.
    pub fn begin(&mut self, revision: u64) -> Option<BuildToken> {
        if self
            .visible_revision()
            .is_some_and(|visible| revision < visible)
        {
            return None;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        let token = BuildToken {
            generation: self.next_generation,
            revision,
        };
        self.building = Some(token);
        Some(token)
    }

    /// Only the newest matching request can replace the representation.
    /// The caller must establish GPU readiness before invoking this function.
    pub fn publish_ready(&mut self, token: BuildToken, value: T) -> bool {
        if self.building != Some(token)
            || self
                .visible_revision()
                .is_some_and(|visible| token.revision < visible)
        {
            return false;
        }
        self.published = Some((token.revision, value));
        self.building = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_completion_cannot_remove_or_replace_visible_data() {
        let mut residency = Residency::default();
        let first = residency.begin(1).unwrap();
        assert!(residency.publish_ready(first, "initial"));
        let stale = residency.begin(2).unwrap();
        let newest = residency.begin(3).unwrap();
        assert_eq!(residency.visible(), Some(&"initial"));
        assert!(!residency.publish_ready(stale, "wrong"));
        assert_eq!(residency.visible(), Some(&"initial"));
        assert!(residency.publish_ready(newest, "new"));
        assert_eq!(residency.visible(), Some(&"new"));
        assert_eq!(residency.visible_revision(), Some(3));
        assert!(residency.begin(2).is_none());
    }

    #[test]
    fn fresh_generation_prevents_reused_revision_from_publishing() {
        let mut residency = Residency::<u32>::default();
        let old = residency.begin(7).unwrap();
        let new = residency.begin(7).unwrap();
        assert!(!residency.publish_ready(old, 10));
        assert!(residency.publish_ready(new, 20));
        assert_eq!(residency.visible(), Some(&20));
    }
}
