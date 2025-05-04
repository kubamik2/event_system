#![feature(ptr_metadata, const_type_id, sync_unsafe_cell, mapped_lock_guards, slice_concat_trait)]
use std::{any::{Any, TypeId}, cell::{Ref, RefMut}, collections::HashMap, ops::Deref};

mod event_handler;
mod event_manager;
pub mod util;
mod maps;

use event_manager::Command;
pub use event_manager::EventManager;
pub use event_handler::EventHandler;
use maps::EventSystemMap;
use util::Delayed;

#[macro_export] macro_rules! handlers {
    ($($f:expr),*) => {
        &[ $(event_system::EventHandler::new($f),)* ]
    };
}

#[macro_export]
macro_rules! dependencies {
    ($($ref:ident $ty:ty),*) => {
        &[$(
            event_system::Dependency::new::<$ty>({
                event_system::util::is_mut(stringify!($ref))                
            })
        ),*]
    };
}

pub trait EventSystem: Send + Sync + Sized {
    const EVENT_HANDLERS: &'static [EventHandler] = &[];
    const DEPENDENCIES: &'static [Dependency] = &[];
    fn event_system_ctx(&self) -> &EventSystemContext<Self>;
}

pub struct EventSystemExecutionPackage<'a> {
    pub event_system: &'a mut (dyn Any + Send + Sync),
    pub executions: Vec<EventHandlerExecution<'a>>,
}

pub struct EventHandlerExecution<'a> {
    pub event_handler: EventHandler,
    pub event: &'a (dyn Any + Send + Sync),
}

#[derive(Clone, Copy)]
pub struct Dependency {
    type_id: TypeId,
    pub mutable: bool,
}

impl Dependency {
    pub const fn new<D: 'static>(mutable: bool) -> Self {
        Self {
            type_id: TypeId::of::<D>(),
            mutable,
        }
    }
}

struct PtrWrapper<T>(*const T);
impl<T> Deref for PtrWrapper<T> {
    type Target = *const T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
unsafe impl<T> Sync for PtrWrapper<T> {}
unsafe impl<T> Send for PtrWrapper<T> {}

#[derive(Clone)]
pub struct EventSender(pub(crate) std::sync::mpsc::Sender<Box<dyn Any + Send + Sync>>);

impl EventSender {
    pub fn send<E: Send + Sync + 'static>(&self, event: E) {
        self.0.send(Box::new(event)).unwrap();
    } 

    pub fn send_delayed<E: Send + Sync + 'static>(&self, event: E) {
        self.0.send(Box::new(Delayed(Box::new(event)))).unwrap();
    } 
}

pub struct EventSystemContext<S: EventSystem> {
    event_system_map: PtrWrapper<EventSystemMap>,
    in_progress_event_sender: EventSender,
    queued_event_sender: EventSender,
    command_sender: std::sync::mpsc::Sender<Command>,
    _t: std::marker::PhantomData<S>,
}

impl<S: EventSystem> EventSystemContext<S> {
    pub fn send<E: 'static + Send + Sync>(&self, event: E) {
        self.in_progress_event_sender.send(event);
    }

    #[inline]
    pub fn send_delayed<E: 'static + Send + Sync>(&self, event: E) {
        self.in_progress_event_sender.send_delayed(event);
    }

    pub fn get<D: EventSystem + 'static>(&self) -> Option<Ref<D>> {
        assert!(S::DEPENDENCIES.iter().any(|f| f.type_id == TypeId::of::<D>()), "'{}' system is not in '{}' system dependencies or is not mutable", std::any::type_name::<D>(), std::any::type_name::<S>());
        unsafe { self.event_system_map.as_ref().unwrap() }.get::<D>()
    }

    pub fn get_mut<D: EventSystem + 'static>(&self) -> Option<RefMut<D>> {
        assert!(S::DEPENDENCIES.iter().any(|f| (f.type_id == TypeId::of::<D>()) && f.mutable), "'{}' system is not in '{}' system dependencies or is not mutable", std::any::type_name::<D>(), std::any::type_name::<S>());
        unsafe { self.event_system_map.as_ref().unwrap() }.get_mut::<D>()
    }

    pub fn event_sender(&self) -> EventSender {
        self.queued_event_sender.clone()
    }

    pub fn add_system<S2: EventSystem + 'static, F: (FnOnce(EventSystemContext<S2>) -> S2) + Send + 'static>(&self, f: F) {
        let _ = self.command_sender.send(Command::AddSystem(
            Box::new(|mgr: &mut EventManager| EventManager::add_system(mgr, f))
        ));
    }

    pub fn remove_system<S2: EventSystem + 'static>(&self) {
        let _ = self.command_sender.send(Command::RemoveSystem(TypeId::of::<S2>()));
    }
}

pub trait ExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>);
}

pub struct SequentialExecutionManager;
impl ExecutionManager for SequentialExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>) {
        for execution_package in execution_packages.into_values() {
            for EventHandlerExecution { event_handler, event }  in execution_package.executions {
                event_handler.execute(execution_package.event_system, event);
            }
        }
    }
}

pub struct RayonExecutionManager {
    pool: rayon::ThreadPool
}

impl RayonExecutionManager {
    pub fn new(num_threads: usize) -> Result<Self, rayon::ThreadPoolBuildError>  {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .use_current_thread()
            .build()?;
        Ok(Self {
            pool,
        })
    }
}

impl ExecutionManager for RayonExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>) {
        self.pool.in_place_scope(move |scope| {
            for execution_package in execution_packages.into_values() {
                scope.spawn(move |_| {
                    for EventHandlerExecution { event_handler, event } in execution_package.executions {
                        event_handler.execute(execution_package.event_system, event);
                    }
                });
            }
        });
    }
}
