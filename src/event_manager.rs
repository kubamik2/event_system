use std::{any::{Any, TypeId}, sync::{mpsc::{channel, Receiver, Sender}, Arc, RwLock}};
use crate::{typemap::ShareTypeMap, Context, SequentialExecutionManager, EventHandlerMap, EventSystem, EventSystemExecutionPackage, ExecutionManager};
use hashbrown::HashMap;

pub struct EventManager {
    state: Arc<RwLock<ShareTypeMap>>,
    handler_map: EventHandlerMap,
    systems: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    queued_receiver: Receiver<Box<dyn Any + Send + Sync>>,
    in_progress_receiver: Receiver<Box<dyn Any + Send + Sync>>,
    queued_sender: Sender<Box<dyn Any + Send + Sync>>,
    in_progress_sender: Sender<Box<dyn Any + Send + Sync>>,
    execution_manager: Box<dyn ExecutionManager>
}

impl Default for EventManager {
    fn default() -> Self {
        let (queued_sender, queued_receiver) = channel();
        let (in_progress_sender, in_progress_receiver) = channel();

        Self {
            state: Arc::new(RwLock::new(ShareTypeMap::default())),
            systems: HashMap::new(),
            handler_map: EventHandlerMap::default(),
            queued_receiver,
            in_progress_receiver,
            queued_sender,
            in_progress_sender,
            execution_manager: Box::new(SequentialExecutionManager),
        }
    }
}

impl EventManager {
    pub fn new_with_execution_manager<M: ExecutionManager + 'static>(execution_manager: M) -> Self {
        Self {
            execution_manager: Box::new(execution_manager),
            ..Default::default()
        }
    }

    pub fn add_system<S: EventSystem + 'static + Send + Sync, F: FnOnce(Context) -> S>(&mut self, f: F) {
        let system = f(self.ctx());

        assert!(self.systems.insert(TypeId::of::<S>(), Box::new(system)).is_none(), "duplicate event system");

        for handler in S::EVENT_HANDLERS.iter().cloned() {
            self.handler_map.push(handler);
        }
    }

    pub fn remove_system<S: EventSystem + 'static>(&mut self) {
        let type_id = TypeId::of::<S>();
        self.handler_map.remove_event_system(type_id);
        self.systems.remove(&type_id);
    }

    #[inline]
    pub fn send<E: 'static + Send + Sync>(&self, event: E) {
        let _ = self.queued_sender.send(Box::new(event));
    }

    fn flush_from_receiver(receiver: &Receiver<Box<dyn Any + Send + Sync>>, systems: &mut HashMap<TypeId, Box<dyn Any + Send + Sync>>, handler_map: &EventHandlerMap, execution_manager: &mut Box<dyn ExecutionManager>) -> usize {
        let mut events_handled = 0;
        let events = receiver.try_iter().collect::<Vec<Box<dyn Any + Send + Sync>>>();
        let mut queried_systems: Vec<Box<dyn Any + Send + Sync>> = Vec::new();
        let mut execution_packages: HashMap<TypeId, EventSystemExecutionPackage> = HashMap::new();

        // collect systems that are needed into a Vec
        for event in events.iter().map(|f| f.as_ref()) {
            let Some(handlers) = handler_map.get(&event.type_id()) else { continue; };
            for handler in handlers {
                if let Some(system) = systems.remove(&handler.event_system_type_id()) {
                    events_handled += 1;
                    queried_systems.push(system);
                }
            }
        }

        // create execution packages for every queried system
        for system in queried_systems.iter_mut().map(|f| f.as_mut()) {
            let type_id = (*system).type_id();
            execution_packages.insert(type_id, EventSystemExecutionPackage {
                event_system: system,
                handler_event_pairs: Vec::new(),
            });
        }

        // add handlers to packages
        for event in events.iter().map(|f| f.as_ref()) {
            let Some(handlers) = handler_map.get(&event.type_id()) else { continue; };
            for handler in handlers {
                let package = execution_packages.get_mut(&handler.event_system_type_id()).expect("execution package event system type id missing");
                package.handler_event_pairs.push((*handler, event));
            }
        }

        // execute packages using provided manager
        execution_manager.execute(execution_packages);
        

        // return systems back
        for system in queried_systems {
            systems.insert((*system).type_id(), system);
        }

        events_handled
    }

    pub fn flush(&mut self) {
        if Self::flush_from_receiver(&self.queued_receiver, &mut self.systems, &self.handler_map, &mut self.execution_manager) > 0 {
            while Self::flush_from_receiver(&self.in_progress_receiver, &mut self.systems, &self.handler_map, &mut self.execution_manager) > 0 {}
        }
    }

    pub(crate) fn ctx(&self) -> Context {
        Context {
            state: self.state.clone(),
            sender: self.in_progress_sender.clone(),
        }
    }
}
