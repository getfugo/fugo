//! Typed ids and the vector they index.
//!
//! Every arena of the model is an [`IdVec<I, T>`] indexed by its own id type, so a `PageId`
//! can never index the language table. Ids are defined by [`define_id!`](crate::define_id);
//! code outside this module never converts an id to `usize` by hand.

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// A typed index into an [`IdVec`].
pub trait Idx: Copy + Ord + Hash + fmt::Debug {
    /// The position this id stands for.
    fn index(self) -> usize;
    /// The id of position `i`.
    ///
    /// # Panics
    /// When `i` does not fit the id's integer type.
    fn from_index(i: usize) -> Self;
}

/// Defines a `Copy` id newtype over an unsigned integer and implements [`Idx`] for it.
///
/// ```ignore
/// define_id!(/// A page of the model.
///            pub PageId(u32));
/// ```
#[macro_export]
macro_rules! define_id {
    ($(#[$meta:meta])* $vis:vis $name:ident($int:ty)) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        $vis struct $name($int);

        impl $name {
            /// The raw integer (for serialisation into views).
            #[must_use]
            pub const fn raw(self) -> $int {
                self.0
            }
            /// The id with the given raw integer.
            #[must_use]
            pub const fn from_raw(raw: $int) -> Self {
                Self(raw)
            }
        }

        impl $crate::id::Idx for $name {
            fn index(self) -> usize {
                usize::try_from(self.0).expect("id fits usize")
            }
            fn from_index(i: usize) -> Self {
                Self(<$int>::try_from(i).unwrap_or_else(|_| {
                    panic!("{} overflow: {i}", stringify!($name))
                }))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

define_id!(
    /// A page of the model (any kind).
    pub PageId(u32)
);
define_id!(
    /// A language, in configuration order (`[0]` is the default language).
    pub LangIdx(u8)
);
define_id!(
    /// An output format, in render order.
    pub FormatId(u8)
);
define_id!(
    /// A media type of the configuration.
    pub MediaTypeId(u16)
);
define_id!(
    /// A resource of the resource store.
    pub ResourceId(u32)
);
define_id!(
    /// A taxonomy of one language.
    pub TaxonomyIdx(u8)
);
define_id!(
    /// A term of one taxonomy.
    pub TermIdx(u32)
);
define_id!(
    /// A queued image operation.
    pub ImageOpId(u64)
);
define_id!(
    /// A partial frame of the render scope.
    pub FrameId(u64)
);
define_id!(
    /// A page-store transaction.
    pub TxnId(u64)
);

/// A `Vec<T>` indexed by the id type `I`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct IdVec<I: Idx, T> {
    items: Vec<T>,
    _id: PhantomData<fn(I) -> I>,
}

impl<I: Idx, T> IdVec<I, T> {
    /// An empty vector.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            _id: PhantomData,
        }
    }

    /// An empty vector with room for `n` items.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            items: Vec::with_capacity(n),
            _id: PhantomData,
        }
    }

    /// Appends `item` and returns its id.
    pub fn push(&mut self, item: T) -> I {
        let id = I::from_index(self.items.len());
        self.items.push(item);
        id
    }

    /// The item of `id`, if it exists.
    #[must_use]
    pub fn get(&self, id: I) -> Option<&T> {
        self.items.get(id.index())
    }

    /// The item of `id`, mutably, if it exists.
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        self.items.get_mut(id.index())
    }

    /// The number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The id the next [`push`](Self::push) returns.
    #[must_use]
    pub fn next_id(&self) -> I {
        I::from_index(self.items.len())
    }

    /// All ids, in order.
    pub fn ids(&self) -> impl DoubleEndedIterator<Item = I> + ExactSizeIterator + use<I, T> {
        (0..self.items.len()).map(I::from_index)
    }

    /// The items, in id order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// The items, mutably, in id order.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.items.iter_mut()
    }

    /// `(id, item)` pairs, in id order.
    pub fn iter_enumerated(
        &self,
    ) -> impl DoubleEndedIterator<Item = (I, &T)> + ExactSizeIterator + '_ {
        self.items
            .iter()
            .enumerate()
            .map(|(i, t)| (I::from_index(i), t))
    }

    /// The items as a slice (position order equals id order).
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// The items as a plain vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<T> {
        self.items
    }
}

impl<I: Idx, T> Default for IdVec<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Idx, T: fmt::Debug> fmt::Debug for IdVec<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter_enumerated()).finish()
    }
}

impl<I: Idx, T> Index<I> for IdVec<I, T> {
    type Output = T;
    fn index(&self, id: I) -> &T {
        &self.items[id.index()]
    }
}

impl<I: Idx, T> IndexMut<I> for IdVec<I, T> {
    fn index_mut(&mut self, id: I) -> &mut T {
        &mut self.items[id.index()]
    }
}

impl<I: Idx, T> From<Vec<T>> for IdVec<I, T> {
    /// Takes the items in order; the id of `v[i]` is `I::from_index(i)`.
    fn from(items: Vec<T>) -> Self {
        // Validates that every position fits the id type.
        if let Some(last) = items.len().checked_sub(1) {
            let _ = I::from_index(last);
        }
        Self {
            items,
            _id: PhantomData,
        }
    }
}

impl<I: Idx, T> FromIterator<T> for IdVec<I, T> {
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl<I: Idx, T: serde::Serialize> serde::Serialize for IdVec<I, T> {
    /// A sequence in id order (the ids are the positions).
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(s)
    }
}

impl<'de, I: Idx, T: serde::Deserialize<'de>> serde::Deserialize<'de> for IdVec<I, T> {
    /// A sequence; the id of the `i`-th item is `I::from_index(i)`.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<T>::deserialize(d).map(Self::from)
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IdVec<I, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<I: Idx, T> IntoIterator for IdVec<I, T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
