//! Stable stack references for callbacks that may replace inventory slots.

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock};
use steel_registry::item_stack::ItemStack;
use steel_utils::locks::SyncRwLock;

/// A Java-style stack reference. Cloning the handle preserves identity;
/// cloning its contents creates an independent stack.
#[derive(Clone, Debug)]
pub struct StackReference(Arc<SyncRwLock<ItemStack>>);

impl StackReference {
    /// Allocates a distinct stack object.
    #[must_use]
    pub fn new(stack: ItemStack) -> Self {
        Self(Arc::new(SyncRwLock::new(stack)))
    }

    /// Copies contents into an independent stack object.
    #[must_use]
    pub fn snapshot(&self) -> ItemStack {
        self.read().clone()
    }

    /// Tests object identity, not item/component equality.
    #[must_use]
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Reads the live stack. Release the guard before calling entity/world hooks.
    pub fn read(&self) -> StackRead<'static> {
        StackRead::Shared(SyncRwLock::read_arc(&self.0))
    }

    /// Mutates the live stack. Release the guard before calling entity/world hooks.
    pub fn write(&self) -> StackWrite<'static> {
        StackWrite::Shared(SyncRwLock::write_arc(&self.0))
    }
}

/// Scoped access to a stack in either owned or reference-backed storage.
#[derive(Debug)]
pub enum StackRead<'a> {
    /// Stack owned by the enclosing container lock.
    Borrowed(&'a ItemStack),
    /// Stack whose identity outlives its inventory slot.
    Shared(ArcRwLockReadGuard<RawRwLock, ItemStack>),
}

impl Deref for StackRead<'_> {
    type Target = ItemStack;
    fn deref(&self) -> &ItemStack {
        match self {
            Self::Borrowed(stack) => stack,
            Self::Shared(stack) => stack,
        }
    }
}

/// Scoped mutable stack access. Assignment edits the referenced object; replace
/// a container slot through its setter to install a different stack object.
#[derive(Debug)]
pub enum StackWrite<'a> {
    /// Stack owned by the enclosing container lock.
    Borrowed(&'a mut ItemStack),
    /// Stack whose identity outlives its inventory slot.
    Shared(ArcRwLockWriteGuard<RawRwLock, ItemStack>),
}

impl Deref for StackWrite<'_> {
    type Target = ItemStack;
    fn deref(&self) -> &ItemStack {
        match self {
            Self::Borrowed(stack) => stack,
            Self::Shared(stack) => stack,
        }
    }
}

impl DerefMut for StackWrite<'_> {
    fn deref_mut(&mut self) -> &mut ItemStack {
        match self {
            Self::Borrowed(stack) => stack,
            Self::Shared(stack) => stack,
        }
    }
}

/// Storage-local view of a container's stack objects.
pub enum StackItems<'a> {
    /// Ordinary owned container storage.
    Owned(&'a [ItemStack]),
    /// Reference-backed container storage.
    Shared(&'a [StackReference]),
}

impl<'a> StackItems<'a> {
    /// Number of slots.
    pub fn len(&self) -> usize {
        match self {
            Self::Owned(items) => items.len(),
            Self::Shared(items) => items.len(),
        }
    }
    /// Whether there are no slots.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Reads a slot without copying its contents.
    pub fn get(&self, slot: usize) -> StackRead<'a> {
        match self {
            Self::Owned(items) => StackRead::Borrowed(&items[slot]),
            Self::Shared(items) => items[slot].read(),
        }
    }
    /// Visits each live stack.
    pub fn iter(self) -> Box<dyn Iterator<Item = StackRead<'a>> + 'a> {
        match self {
            Self::Owned(items) => Box::new(items.iter().map(StackRead::Borrowed)),
            Self::Shared(items) => {
                Box::new(items.iter().map(|item| -> StackRead<'a> { item.read() }))
            }
        }
    }
}

/// Mutable storage view; slot replacement is distinct from object mutation.
pub enum StackItemsMut<'a> {
    /// Ordinary owned container storage.
    Owned(&'a mut [ItemStack]),
    /// Reference-backed container storage.
    Shared(&'a mut [StackReference]),
}

impl<'a> StackItemsMut<'a> {
    /// Mutates an existing stack object.
    pub fn get(self, slot: usize) -> StackWrite<'a> {
        match self {
            Self::Owned(items) => StackWrite::Borrowed(&mut items[slot]),
            Self::Shared(items) => items[slot].write(),
        }
    }
    /// Installs a new stack object in a slot.
    pub fn set(&mut self, slot: usize, stack: ItemStack) {
        match self {
            Self::Owned(items) => items[slot] = stack,
            Self::Shared(items) => items[slot] = StackReference::new(stack),
        }
    }
    /// Visits mutable stack objects without replacing their identities.
    pub fn iter_mut(self) -> Box<dyn Iterator<Item = StackWrite<'a>> + 'a> {
        match self {
            Self::Owned(items) => Box::new(items.iter_mut().map(StackWrite::Borrowed)),
            Self::Shared(items) => {
                Box::new(items.iter().map(|item| -> StackWrite<'a> { item.write() }))
            }
        }
    }
    /// Borrows distinct slots together.
    pub fn with_indices<const N: usize>(self, indices: [usize; N]) -> [StackWrite<'a>; N] {
        match self {
            Self::Owned(items) => {
                let Ok(items) = items.get_disjoint_mut(indices) else {
                    panic!("stack indices must be distinct and in bounds")
                };
                items.map(StackWrite::Borrowed)
            }
            Self::Shared(items) => {
                for (position, index) in indices.iter().enumerate() {
                    assert!(
                        *index < items.len() && !indices[..position].contains(index),
                        "stack indices must be distinct and in bounds"
                    );
                    assert!(
                        indices[..position]
                            .iter()
                            .all(|other| !items[*index].is_same(&items[*other])),
                        "mutable slots must refer to distinct stack objects"
                    );
                }
                indices.map(|index| items[index].write())
            }
        }
    }
}

impl PartialEq for StackRead<'_> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl PartialEq<ItemStack> for StackRead<'_> {
    fn eq(&self, other: &ItemStack) -> bool {
        **self == *other
    }
}

impl PartialEq<&ItemStack> for StackRead<'_> {
    fn eq(&self, other: &&ItemStack) -> bool {
        **self == **other
    }
}
