use std::{mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

pub struct Consumable<T> {
    is_consumed: AtomicBool,
    inner: MaybeUninit<T>,
}

impl<T> std::fmt::Debug for Consumable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.display_fmt(f) }
}

impl<T> std::fmt::Display for Consumable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.display_fmt(f) }
}

unsafe impl<T> Sync for Consumable<T> {}
unsafe impl<T> Send for Consumable<T> {}

impl<T> Consumable<T> {
    pub fn new(inner: T) -> Self {
        Self {
            is_consumed: AtomicBool::new(false),
            inner: MaybeUninit::new(inner),
        }
    }

    pub fn consume(&self) -> Option<T> {
        let is_consumed = self.is_consumed.swap(true, Ordering::Relaxed);
        if is_consumed { return None; }

        Some(unsafe { self.inner.assume_init_read() })
    }

    pub fn is_consumed(&self) -> bool {
        self.is_consumed.load(Ordering::Relaxed)
    }

    fn display_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let is_consumed = self.is_consumed.load(Ordering::Relaxed);
        f.debug_tuple(std::any::type_name::<Self>())
            .field(&if is_consumed { "Consumed" } else { "Unconsumed" })
            .finish()
    }
}
