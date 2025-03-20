#![feature(ptr_metadata)]
use std::{any::{Any, TypeId}, sync::{Arc, RwLock}};
use typemap::ShareTypeMap;
use hashbrown::{HashMap, hash_map::Entry};

pub mod event_handler;
pub mod event_manager;
pub mod typemap;
pub mod util;

pub use event_manager::EventManager;
pub use event_handler::EventHandler;

#[macro_export]
macro_rules! handlers {
    ($($f:expr),*) => {
        &[ $(event_system::EventHandler::new($f),)* ]
    };
}

pub trait EventSystem {
    const EVENT_HANDLERS: &'static [EventHandler];
}

pub struct EventSystemExecutionPackage<'a> {
    pub event_system: &'a mut (dyn Any + Send + Sync),
    pub handler_event_pairs: Vec<(EventHandler, &'a (dyn Any + Send + Sync))>,
}

#[derive(Default)]
struct EventHandlerMap(HashMap<TypeId, Vec<EventHandler>>);

impl EventHandlerMap {
    fn get(&self, event_type_id: &TypeId) -> Option<&Vec<EventHandler>> {
        self.0.get(event_type_id)
    }

    fn push(&mut self, handler: EventHandler) {
        match self.0.entry(handler.event_type_id()) {
            Entry::Vacant(vacant) => {
                vacant.insert(vec![handler]);
            },
            Entry::Occupied(mut occupied) => {
                occupied.get_mut().push(handler);
            }
        }
    }

    fn remove_event_system(&mut self, event_system_type_id: TypeId) {
        for handlers in self.0.values_mut() {
            handlers.retain(|handler| handler.event_system_type_id() != event_system_type_id);
        }
    }
}

pub struct Context {
    state: Arc<RwLock<ShareTypeMap>>,
    read_only_state: Arc<ShareTypeMap>,
    sender: std::sync::mpsc::Sender<Box<dyn Any + Send + Sync>>,
}

impl Context {
    pub fn send<E: 'static + Send + Sync>(&self, event: E) {
        let _ = self.sender.send(Box::new(event));
    }

    pub fn state(&self) -> &Arc<RwLock<ShareTypeMap>> {
        &self.state
    }

    pub fn read_only_state(&self) -> &Arc<ShareTypeMap> {
        &self.read_only_state
    }
}

pub trait ExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>);
}

pub struct SequentialExecutionManager;
impl ExecutionManager for SequentialExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>) {
        for execution_package in execution_packages.into_values() {
            for (handler, event) in execution_package.handler_event_pairs {
                handler.execute(execution_package.event_system, event);
            }
        }
    }
}

pub struct RayonExecutionManager {
    pool: rayon::ThreadPool
}

impl RayonExecutionManager {
    pub fn new(num_threads: usize) -> Result<Self, rayon::ThreadPoolBuildError>  {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(num_threads).build()?;
        Ok(Self {
            pool,
        })
    }
}

impl ExecutionManager for RayonExecutionManager {
    fn execute(&mut self, execution_packages: HashMap<TypeId, EventSystemExecutionPackage>) {
        self.pool.scope(|scope| {
            for execution_package in execution_packages.into_values() {
                scope.spawn(|_| {
                    for (handler, event) in execution_package.handler_event_pairs {
                        handler.execute(execution_package.event_system, event);
                    }
                });
            }
        });
    }
}
