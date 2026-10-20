//! A value that can be replaced while readers use it (D-300): the integrations policy an
//! administrator saves applies without a restart. Readers call [`Live::load`] on every use and
//! get a cheap snapshot; a writer calls [`Live::store`]. In-process only (one node).

use std::fmt;
use std::sync::{Arc, RwLock};

/// A shared, replaceable `Arc<T>`.
pub struct Live<T>(Arc<RwLock<Arc<T>>>);

impl<T> Live<T> {
    /// A handle holding `value`.
    pub fn new(value: T) -> Self {
        Self(Arc::new(RwLock::new(Arc::new(value))))
    }

    /// The current value. Never blocks for long: a writer holds the lock only to swap a pointer.
    pub fn load(&self) -> Arc<T> {
        self.0.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Replace the value; every clone of this handle sees it on its next `load`.
    pub fn store(&self, value: T) {
        *self.0.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(value);
    }
}

impl<T> Clone for Live<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Default> Default for Live<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Live<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Live").field(&*self.load()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_is_visible_to_every_clone() {
        let a = Live::new(vec![1]);
        let b = a.clone();
        assert_eq!(*b.load(), vec![1]);
        a.store(vec![2, 3]);
        assert_eq!(*b.load(), vec![2, 3]);
    }
}
