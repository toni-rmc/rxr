use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, Observer,
};

pub fn generate_u32_observable(
    end: u32,
    last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
) -> Observable<u32> {
    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(i) = rx.recv() {
                *done_c.lock().unwrap() = i;
            }
        });

        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = std::thread::spawn(move || {
            let mut last_emit = 0;

            for i in 0..=end {
                if *done.lock().unwrap() == true {
                    break;
                }
                last_emit = i;
                o.next(i);
                // Important. Put an await point after each emit.
                std::thread::sleep(Duration::from_millis(1));
            }
            o.complete();
            last_emit_assert.lock().unwrap()(last_emit);
        });

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                if let Err(_) = tx.send(true) {
                    eprintln!("receiver dropped");
                }
            })),
            SubscriptionHandle::JoinThread(jh),
        )
    })
}
