use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

use event_system::{event_handler::EventHandler, event_manager::EventManager, Context, EventSystem};
use event_system::handlers;

struct Msg;

struct A {
    ctx: Context,
    counter: usize
}

impl A {
    fn ping(&mut self, event: &Arc<AtomicBool>) {
        if event.load(Ordering::Relaxed) {
            println!("A thread id: {:?}", std::thread::current().id());
            self.counter += 1;
            // for _ in 0..1000000 {
                self.ctx.send(Msg);
            // }
        }

        // println!("A {}", self.counter);
    }
}

impl EventSystem for A {
    const EVENT_HANDLERS: &'static [EventHandler] = handlers!(Self::ping);
}

struct AParity {
    counter: usize,
}

impl AParity {
    fn ping(&mut self, _: &Msg) {
        println!("Main thread id: {:?}", std::thread::current().id());
        self.counter += 1;
        // dbg!(self.counter);
    }
}

impl EventSystem for AParity {
    const EVENT_HANDLERS: &'static [EventHandler] = &[EventHandler::new(AParity::ping)];
}

fn main() {
    // println!("Main thread id: {:?}", std::thread::current().id());
    // let mut ctx = EventManager::default();
    // let x = Arc::new(AtomicBool::new(true));
    // ctx.add_system(|ctx| A { ctx, counter: 0});
    // ctx.add_system(|_| AParity { counter: 0});

    // let now = std::time::Instant::now();
    // for _ in 0..1 {
    //     ctx.send(x.clone());
    //     ctx.send(Msg);
    //     ctx.flush();
    // }
    // println!("Time taken: {:?}", now.elapsed());

    let mut manager = EventManager::default();
    manager.add_system(|ctx| A { ctx, counter: 0 });
    manager.add_system(|_| AParity { counter: 0 });

    manager.remove_system::<A>();
}
