use std::{any::{Any, TypeId}, cell::{Ref, RefCell, RefMut}};
use std::collections::{HashMap, hash_map::Entry, HashSet};
use crate::{event_handler::EventHandler, Dependency, EventSystem};

#[derive(Default)]
pub(crate) struct EventToHandlersMap(HashMap<TypeId, Vec<EventHandler>>);

impl EventToHandlersMap {
    pub(crate) fn get(&self, event_type_id: &TypeId) -> Option<&Vec<EventHandler>> {
        self.0.get(event_type_id)
    }

    pub(crate) fn push(&mut self, handler: EventHandler) {
        match self.0.entry(handler.event_type_id()) {
            Entry::Vacant(vacant) => {
                vacant.insert(vec![handler]);
            },
            Entry::Occupied(mut occupied) => {
                occupied.get_mut().push(handler);
            }
        }
    }

    pub(crate) fn remove_event_system(&mut self, event_system_type_id: &TypeId) {
        for handlers in self.0.values_mut() {
            handlers.retain(|handler| handler.event_system_type_id() != *event_system_type_id);
        }
    }
}

pub(crate) struct TypeErasedEventSystem {
    pub event_system: RefCell<Box<dyn Any + Send + Sync>>,
    pub dependencies: &'static [Dependency],
}

#[derive(Default)]
pub(crate) struct EventSystemMap(HashMap<TypeId, TypeErasedEventSystem>);

impl EventSystemMap {
    pub(crate) fn insert<S: EventSystem + Send + Sync + 'static>(&mut self, event_system: S) -> Option<TypeErasedEventSystem> {
        self.0.insert(TypeId::of::<S>(), TypeErasedEventSystem {
            event_system: RefCell::new(Box::new(event_system)),
            dependencies: S::DEPENDENCIES,
        })
    }

    pub(crate) fn remove(&mut self, type_id: &TypeId) -> Option<TypeErasedEventSystem> {
        self.0.remove(type_id)
    }

    pub(crate) fn get_raw(&self, event_system_type_id: &TypeId) -> Option<&TypeErasedEventSystem> {
        self.0.get(event_system_type_id)
    }

    pub(crate) fn get<S: EventSystem + 'static>(&self) -> Option<Ref<S>> {
        self.0
            .get(&TypeId::of::<S>())
            .map(|f| {
                Ref::map(f.event_system.borrow(), |f| f.downcast_ref().unwrap())
            })
    }

    pub(crate) fn get_mut<S: EventSystem + 'static>(&self) -> Option<RefMut<S>> {
        self.0
            .get(&TypeId::of::<S>())
            .map(|f| {
                RefMut::map(f.event_system.borrow_mut(), |f| f.downcast_mut().unwrap())
            })
    }

    pub(crate) fn contains<S: EventSystem + 'static>(&self) -> bool {
        self.0.contains_key(&TypeId::of::<S>())
    }

}

#[derive(Default)]
pub(crate) struct HandlerMap(HashMap<(TypeId, TypeId), EventHandler>);

impl HandlerMap {
    pub(crate) fn insert(&mut self, handler: EventHandler) -> Option<EventHandler> {
        self.0.insert((handler.event_system_type_id(), handler.event_type_id()), handler)
    }

    pub(crate) fn get(&self, event_system_type_id: TypeId, event_type_id: TypeId) -> Option<&EventHandler> {
        self.0.get(&(event_system_type_id, event_type_id))
    }

    pub(crate) fn remove_event_system(&mut self, event_system_type_id: &TypeId) {
        self.0.retain(|(f, _), _| {
            *f != *event_system_type_id
        });
    }
}

#[derive(Default)]
pub(crate) struct CompatibilityMap(HashSet<(TypeId, TypeId)>);

impl CompatibilityMap {
    pub(crate) fn insert<S: EventSystem + 'static>(&mut self, event_system_map: &EventSystemMap) {
        let type_id = TypeId::of::<S>();
        self.0.insert((type_id, type_id));
        'outer: for (event_system_type_id, type_erased_system) in event_system_map.0.iter() {
            let system_a = type_id.max(*event_system_type_id);
            let system_b = type_id.min(*event_system_type_id);

            if type_erased_system.dependencies.iter().any(|f| f.type_id == type_id) {
                continue 'outer;
            }
            if S::DEPENDENCIES.iter().any(|f| f.type_id == *event_system_type_id) {
                continue 'outer;
            }

            for dep in S::DEPENDENCIES.iter() {
                let Some(other_dep) = type_erased_system.dependencies.iter().find(|f| f.type_id == dep.type_id) else { continue; };
                if dep.mutable | other_dep.mutable {
                    continue 'outer;
                }
            }
            self.0.insert((system_a, system_b));
        }
    }

    pub(crate) fn remove(&mut self, type_id: &TypeId) {
        self.0.retain(|(a, b)| !(*a == *type_id || *b == *type_id));
    }

    pub(crate) fn is_compatible(&self, event_system_a_type_id: TypeId, event_system_b_type_id: TypeId) -> bool {
        let system_a = event_system_a_type_id.max(event_system_b_type_id);
        let system_b = event_system_a_type_id.min(event_system_b_type_id);
        self.0.contains(&(system_a, system_b))
    }
}
