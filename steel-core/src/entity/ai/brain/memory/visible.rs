//! Lazy visibility results cached for one sensor scan.

use super::{MemoryValue, RememberedEntities};
use crate::entity::SharedEntity;
use rustc_hash::FxHashMap;
use std::fmt;
use steel_utils::{DowncastType, DowncastTypeKey, locks::SyncMutex};
use uuid::Uuid;

type VisibilityTest = Box<dyn Fn(&SharedEntity) -> bool + Send + Sync>;

pub struct NearestVisibleLivingEntities {
    nearby: RememberedEntities,
    test: VisibilityTest,
    cache: SyncMutex<FxHashMap<Uuid, bool>>,
}

// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for NearestVisibleLivingEntities {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/visible_entities");
}
impl MemoryValue for NearestVisibleLivingEntities {}
impl fmt::Debug for NearestVisibleLivingEntities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NearestVisibleLivingEntities")
            .field("nearby", &self.nearby)
            .finish_non_exhaustive()
    }
}
impl NearestVisibleLivingEntities {
    /// The empty visibility result used before a mob's first sensor scan.
    pub fn empty() -> &'static Self {
        static EMPTY: std::sync::LazyLock<NearestVisibleLivingEntities> =
            std::sync::LazyLock::new(|| {
                NearestVisibleLivingEntities::new(RememberedEntities::default(), |_| false)
            });
        &EMPTY
    }
    pub fn new(
        nearby: RememberedEntities,
        test: impl Fn(&SharedEntity) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            nearby,
            test: Box::new(test),
            cache: SyncMutex::default(),
        }
    }
    pub fn nearby_entities(&self) -> &RememberedEntities {
        &self.nearby
    }
    pub fn find_closest(&self, filter: impl Fn(&SharedEntity) -> bool) -> Option<SharedEntity> {
        self.nearby
            .resolve()
            .find(|entity| filter(entity) && self.is_visible(entity))
    }
    pub fn contains(&self, target: &SharedEntity) -> bool {
        self.nearby
            .resolve()
            .any(|entity| std::sync::Arc::ptr_eq(&entity, target))
            && self.is_visible(target)
    }
    fn is_visible(&self, entity: &SharedEntity) -> bool {
        if let Some(visible) = self.cache.lock().get(&entity.uuid()).copied() {
            return visible;
        }
        let visible = (self.test)(entity);
        self.cache.lock().insert(entity.uuid(), visible);
        visible
    }
}
