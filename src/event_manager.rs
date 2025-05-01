use std::{any::{Any, TypeId}, cell::{Ref, RefMut}, collections::{HashMap, HashSet}, sync::mpsc::{channel, Receiver, Sender}};
use crate::{maps::*, util::Delayed, EventHandler, EventHandlerExecution, EventSystem, EventSystemContext, EventSystemExecutionPackage, EventSystemMap, ExecutionManager, SequentialExecutionManager};

pub struct EventManager {
    event_to_handlers_map: EventToHandlersMap,
    handler_map: HandlerMap,
    compatibility_map: CompatibilityMap,
    event_system_map: EventSystemMap,

    in_progress_receiver: Receiver<Box<dyn Any + Send + Sync>>,
    in_progress_sender: Sender<Box<dyn Any + Send + Sync>>,

    queued_receiver: Receiver<Box<dyn Any + Send + Sync>>,
    queued_sender: Sender<Box<dyn Any + Send + Sync>>,

    command_receiver: Receiver<Command>,
    command_sender: Sender<Command>,

    execution_manager: Box<dyn ExecutionManager>,
}

impl Default for EventManager {
    fn default() -> Self {
        let (queued_sender, queued_receiver) = channel();
        let (in_progress_sender, in_progress_receiver) = channel();
        let (command_sender, command_receiver) = channel();

        Self {
            event_system_map: Default::default(),
            handler_map: Default::default(),
            compatibility_map: Default::default(),
            event_to_handlers_map: EventToHandlersMap::default(),
            queued_receiver,
            in_progress_receiver,
            queued_sender,
            in_progress_sender,
            command_sender,
            command_receiver,
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

    pub fn add_system<S: EventSystem + 'static, F: FnOnce(EventSystemContext<S>) -> S>(&mut self, f: F) {
        let system = f(self.create_ctx());

        assert!(self.event_system_map.insert(system).is_none(), "duplicate event system");

        for handler in S::EVENT_HANDLERS.iter().cloned() {
            self.event_to_handlers_map.push(handler);
            assert!(self.handler_map.insert(handler).is_none(), "duplicate event handler");
        }

        self.compatibility_map.insert::<S>(&self.event_system_map);
        self.process_commands();
    }

    pub fn remove_system<S: EventSystem + 'static>(&mut self) {
        let type_id = TypeId::of::<S>();
        self.remove_system_by_type_id(&type_id);
    }

    fn remove_system_by_type_id(&mut self, type_id: &TypeId) {
        self.event_to_handlers_map.remove_event_system(type_id);
        self.event_system_map.remove(type_id);
        self.compatibility_map.remove(type_id);
        self.handler_map.remove_event_system(type_id);
    }

    pub fn contains_system<S: EventSystem + 'static>(&self) -> bool {
        self.event_system_map.contains::<S>()
    }

    pub fn get<S: EventSystem + 'static>(&self) -> Option<Ref<S>> {
        self.event_system_map.get::<S>()
    }

    pub fn get_mut<S: EventSystem + 'static>(&self) -> Option<RefMut<S>> {
        self.event_system_map.get_mut::<S>()
    }

    #[inline]
    pub fn send<E: 'static + Send + Sync>(&self, event: E) {
        let _ = self.queued_sender.send(Box::new(event));
    }

    #[inline]
    pub fn send_delayed<E: 'static + Send + Sync>(&self, event: E) {
        let _ = self.queued_sender.send(Box::new(Delayed(Box::new(event))));
    }

    fn flush_from_receiver(&mut self, in_progress_receiver: bool) -> usize {
        fn add_execution(handler: &EventHandler, event_system_map: &EventSystemMap, execution: &mut Vec<(EventHandler, *const (dyn Any + Send + Sync))>, handler_map: &HandlerMap, added_handlers: &mut HashSet<(TypeId, TypeId)>, event: &(dyn Any + Send + Sync)) {
            if added_handlers.contains(&(handler.event_system_type_id(), handler.event_type_id())) { return; }
            for dep in event_system_map.get_raw(&handler.event_system_type_id()).unwrap().dependencies.iter() {
                if let Some(dep_handler) = handler_map.get(dep.type_id, handler.event_type_id()) {
                    add_execution(dep_handler, event_system_map, execution, handler_map, added_handlers, event);
                }
            }
            added_handlers.insert((handler.event_system_type_id(), handler.event_type_id()));
            execution.push((*handler, event));
        }
        
        let receiver = {
            if in_progress_receiver {
                &self.in_progress_receiver
            } else {
                &self.queued_receiver
            }
        };

        let mut events_handled = 0;
        let mut events = vec![];
        let mut delayed_events = vec![];
        let mut execution: Vec<(EventHandler, *const (dyn Any + Send + Sync))> = Default::default();

        // form linear execution of handlers for each event while maintaining invariants
        for event in receiver.try_iter() {
            let event_type_id = event.as_ref().type_id();

            if event_type_id == TypeId::of::<Delayed>() {
                delayed_events.push(event.downcast::<Delayed>().unwrap());
                continue;
            }

            events_handled += 1;
            let mut added_handlers = Default::default();
            let Some(handlers) = self.event_to_handlers_map.get(&event_type_id) else { continue; };
            for handler in handlers {
                add_execution(handler, &self.event_system_map, &mut execution, &self.handler_map, &mut added_handlers, event.as_ref());
            }
            events.push(event);
        }

        for event in delayed_events {
            self.in_progress_sender.send(event.0).unwrap();
        }

        let mut parallel_execution: Vec<Vec<(EventHandler, *const (dyn Any + Send + Sync))>> = vec![vec![]];
        let mut level = 0;

        // turn linear execution into parallel execution
        for i in 0..(execution.len().saturating_sub(1)) {
            parallel_execution[level].push(execution[i]);
            if !self.compatibility_map.is_compatible(execution[i].0.event_system_type_id(), execution[i+1].0.event_system_type_id()) {
                level += 1;
                parallel_execution.push(vec![]);
            }
        }
        if let Some(handler) = execution.last().copied() {
            parallel_execution[level].push(handler);
        }

        // execute
        for execution in parallel_execution {
            let mut execution_packages: HashMap<TypeId, EventSystemExecutionPackage> = Default::default();
            for (handler, event) in execution {
                execution_packages.entry(handler.event_system_type_id()).or_insert(
                    EventSystemExecutionPackage {
                        event_system: unsafe {
                            self.event_system_map
                                .get_raw(&handler.event_system_type_id())
                                .expect("dangling handler")
                                .event_system
                                    .as_ptr()
                                    .as_mut()
                                    .unwrap()
                                    .as_mut() 
                        },
                        executions: vec![]
                    }
                ).executions.push(EventHandlerExecution { event_handler: handler, event: unsafe { event.as_ref().unwrap() } });
            }
            self.execution_manager.execute(execution_packages);
            self.process_commands();
        }

        events_handled
    }

    pub fn process_commands(&mut self) {
        for command in self.command_receiver.try_iter().collect::<Vec<Command>>() {
            match command {
                Command::AddSystem(f) => {
                    f(self);
                    self.process_commands();
                },
                Command::RemoveSystem(type_id) => {
                    self.remove_system_by_type_id(&type_id);
                    self.process_commands();
                }
            }
        }
    }

    pub fn flush(&mut self) -> usize {
        let mut events = 0;
        let mut flush = |in_progress_receiver: bool| {
            let i = self.flush_from_receiver(in_progress_receiver);
            events += 1;
            i
        };
        while flush(true) > 0 {}
        if flush(false) > 0 {
            while flush(true) > 0 {}
        }
        events
    }

    pub fn create_ctx<S: EventSystem + 'static>(&self) -> EventSystemContext<S> {
        EventSystemContext {
            event_system_map: crate::PtrWrapper(&self.event_system_map),
            in_progress_event_sender: crate::EventSender(self.in_progress_sender.clone()),
            queued_event_sender: crate::EventSender(self.queued_sender.clone()),
            command_sender: self.command_sender.clone(),
            _t: Default::default(),
        }
    }

    pub fn set_execution_manager<M: ExecutionManager + 'static>(&mut self, execution_manager: M) {
        self.execution_manager = Box::new(execution_manager);
    }

    pub fn event_sender(&self) -> crate::EventSender {
        crate::EventSender(self.in_progress_sender.clone())
    }
}

pub(crate) enum Command {
    AddSystem(Box<dyn FnOnce(&mut EventManager) + Send>),
    RemoveSystem(TypeId),
}
