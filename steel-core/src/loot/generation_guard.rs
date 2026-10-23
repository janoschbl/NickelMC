use std::{cell::Cell, marker::PhantomData, rc::Rc};

use super::{LootError, LootResult};

thread_local! {
    static GENERATING: Cell<bool> = const { Cell::new(false) };
}

/// Serial stream locks may wait for other workers, but never for this thread.
/// Handwritten tables must evaluate nested tables using their existing context,
/// not start another world/sequence draw or unpack another pending container.
pub(crate) struct LootGenerationGuard(PhantomData<Rc<()>>);

impl LootGenerationGuard {
    pub(crate) fn enter() -> LootResult<Self> {
        if GENERATING.with(|active| active.replace(true)) {
            return Err(LootError::RandomUnavailable("recursive loot generation"));
        }
        Ok(Self(PhantomData))
    }

    pub(crate) fn is_active() -> bool {
        GENERATING.with(Cell::get)
    }
}

impl Drop for LootGenerationGuard {
    fn drop(&mut self) {
        GENERATING.with(|active| active.set(false));
    }
}
