use std::{any::Any, mem::MaybeUninit, ops::{Deref, DerefMut}, sync::atomic::{AtomicBool, Ordering}};

use crate::{EventSystemContext, EventSystem};

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

pub struct Var<T: Send + Sync + 'static> {
    ctx: EventSystemContext<Self>,
    val: T,
    changed: bool,
}

impl<T: Send + Sync + 'static> Deref for Var<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.val
    }
}

impl<T: Send + Sync + 'static> DerefMut for Var<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if !self.changed {
            self.ctx.send(Changed::<T>::default());
            self.changed = true;
        }
        &mut self.val
    }
}

impl<T: Send + Sync + 'static> Var<T> {
    pub fn new(ctx: EventSystemContext<Self>, val: T) -> Self {
        Self {
            ctx,
            val,
            changed: false,
        }
    }

    fn on_changed(&mut self, _: &Changed<T>) {
        self.changed = false;
    }
}

impl<T: Send + Sync + 'static> EventSystem for Var<T> {
    const EVENT_HANDLERS: &'static [crate::EventHandler] = &[crate::EventHandler::new(Self::on_changed)];
    fn event_system_ctx(&self) -> &EventSystemContext<Self> {
        &self.ctx
    }
}

pub struct Changed<T>(std::marker::PhantomData<T>);

impl<T> Default for Changed<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

pub const fn is_mut(s: &'static str) -> bool {
    let mut i = 0;
    while i < s.len() && i < "ref".len() {
        if s.as_bytes()[i] != b"ref"[i] { break; }
        i += 1;
        if i == "ref".len() {
            return false;
        }
    } 

    i = 0;
    while i < s.len() && i < "mut".len() {
        if s.as_bytes()[i] != b"mut"[i] { break; }
        i += 1;
        if i == "mut".len() {
            return true;
        }
    } 

    panic!()
}

pub(crate) struct Delayed(pub(crate) Box<dyn Any + Send + Sync>);
