use std::any::{Any, TypeId};
use crate::EventSystem;

pub type EventHandlerFunction<S, E> = fn(&mut S, &E);

#[derive(Clone, Copy)]
pub struct EventHandler {
    wrapper: unsafe fn(*mut (), &dyn Any, *const ()),
    handler: SyncRawPtr,
    event_type_id: fn() -> TypeId,
    event_system_type_id: fn() -> TypeId,
}

#[derive(Clone, Copy)]
struct SyncRawPtr(pub *const ());
unsafe impl Sync for SyncRawPtr {}
unsafe impl Send for SyncRawPtr {}

impl EventHandler {
    pub const fn new<S: EventSystem + 'static, E: 'static>(function: EventHandlerFunction<S,E>) -> Self {
        let event_type_id = || TypeId::of::<E>();
        let event_system_id = || TypeId::of::<S>();
        Self {
            wrapper: Self::wrapper::<E>,
            handler: SyncRawPtr(function as *const ()),
            event_type_id,
            event_system_type_id: event_system_id
        }
    }

    #[inline]
    unsafe fn wrapper<E: 'static>(system: *mut (), value: &dyn Any, handler: *const ()) {
        unsafe { std::mem::transmute::<*const (), fn(*mut (), &E)>(handler)(system, value.downcast_ref().unwrap_unchecked()) }
    }

    #[inline]
    unsafe fn execute_raw(&self, system: *mut (), value: &dyn Any) {
        unsafe { (self.wrapper)(system, value, self.handler.0) };
    }

    #[inline]
    pub fn execute(&self, system: &mut dyn Any, event: &dyn Any) {
        debug_assert!((*system).type_id() == (self.event_system_type_id)());
        debug_assert!((*event).type_id() == (self.event_type_id)());
        let system_raw_ptr = (system as *mut dyn Any).to_raw_parts().0;
        unsafe { self.execute_raw(system_raw_ptr, event) };
    }

    #[inline]
    pub fn event_type_id(&self) -> TypeId {
        (self.event_type_id)()
    }

    #[inline]
    pub fn event_system_type_id(&self) -> TypeId {
        (self.event_system_type_id)()
    }
}

impl std::fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("event_system_type_id", &self.event_system_type_id())
            .field("event_type_id", &self.event_type_id())
            .finish()
    }
}
