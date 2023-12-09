//! Demonstrating the `take_until()` operator with a notifier observable using OS threads
//!
//! This example illustrates the functionality of the `take_until()` operator by
//! creating a stream and emitting elements until a notifier observable emits a signal.
//!
//! To run this example, execute `cargo run --example take_until_operator_os`.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subject, Subscribeable,
};

const UNSUBSCRIBE_SIGNAL: bool = true;

fn main() {
    let observable = Observable::new(|mut o| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(UNSUBSCRIBE_SIGNAL) = rx.recv() {
                *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
            }
        });

        let join_handle = std::thread::spawn(move || {
            for i in 0..100 {
                if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
                    break;
                }
                o.next(i);
                std::thread::sleep(Duration::from_millis(1));
            }
            o.complete();
        });

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                if tx.send(UNSUBSCRIBE_SIGNAL).is_err() {
                    println!("Receiver dropped.");
                }
            })),
            SubscriptionHandle::JoinThread(join_handle),
        )
    });

    let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
    observer.on_complete(|| println!("Completed"));

    // Create notifier, it can be observable or one of the Subject variants.
    let (mut emitter, receiver) = Subject::emitter_receiver();

    // We can chain the Subject, here we use `delay()` to slow down the notifier so
    // source observable has time to emmit some values. Note when we chain the
    // notifier with operators we don't have to use `into()`. To continue using the
    // receiver later, utilize `.clone()`, e.g. `receiver.clone().delay(20)`.
    let subscription = observable
        .take_until(receiver.delay(20), false)
        .subscribe(observer);

    // Allowing some time for the `take_until` function to register the notifier
    // before emitting a signal. This step is unnecessary if you're not immediately
    // sending a signal.
    std::thread::sleep(Duration::from_millis(1));

    // Send signal to stop source observable emitting values.
    emitter.next(());

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for observable to finish before exiting the program.
    if subscription.join().is_err() {
        // Handle error
    }

    println!("`main` function done")
}
