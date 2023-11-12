/**
 * This `Observable` emits values and completes, returning a `Subscription` that can be
 * unsubscribed from, enabling all operators to function correctly. It utilizes an OS
 * thread for asynchronous processing, preventing it from blocking the current thread.
 */
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic, Unsubscribeable},
    Observable, ObservableExt, Observer, Subscribeable,
};

const UNSUBSCRIBE_SIGNAL: bool = true;

fn main() {
    // Create a custom observable that emits values in a separate thread.
    let observable = Observable::new(|mut o| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn a new thread to await a signal sent from the unsubscribe logic.
        std::thread::spawn(move || {
            // Attempt to receive a signal sent from the unsubscribe logic.
            if let Ok(UNSUBSCRIBE_SIGNAL) = rx.recv() {
                // Update the `done_c` mutex with the received signal.
                *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
            }
        });

        // Launch a new thread for the Observable's processing and store its handle.
        let join_handle = std::thread::spawn(move || {
            for i in 0..=10000 {
                // If an unsubscribe signal is received, exit the loop and stop emissions.
                if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
                    break;
                }
                println!("----------- {}", i);
                // Emit the value to the subscriber.
                o.next(i);
                // Important. Put an await point after each emit or after some emits.
                // This allows the `take()` operator to function properly.
                std::thread::sleep(Duration::from_millis(1));
            }
            // Signal completion to the subscriber.
            o.complete();
        });

        // Return a new `Subscription` with custom unsubscribe logic.
        Subscription::new(
            // The provided closure defines the behavior of the subscription when it
            // is unsubscribed. In this case, it sends a signal to an asynchronous
            // observable to stop emitting values.
            UnsubscribeLogic::Logic(Box::new(move || {
                if tx.send(UNSUBSCRIBE_SIGNAL).is_err() {
                    println!("Receiver dropped.");
                }
            })),
            // Store the `JoinHandle` for awaiting completion using the `Subscription`.
            SubscriptionHandle::JoinThread(join_handle),
        )
    });

    // Create the `Subscriber` with a mandatory `next` function, and optional
    // `complete` function. No need for `error` function in this simple example.
    let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
    observer.on_complete(|| println!("Completed"));

    // This observable uses OS threads so it will not block the current thread.
    // Observables are cold so if you comment out the statement bellow nothing
    // will be emitted.
    let subscription = observable
        // take utilizes our unsubscribe function to stop background emissions after
        // a specified item count.
        .take(500)
        .map(|v| format!("Mapped {}", v))
        .subscribe(observer);

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Unsubscribe from the observable to stop emissions.
    subscription.unsubscribe();

    // Allow some time for the main thread to confirm that the observable indeed
    // isn't emitting.
    std::thread::sleep(Duration::from_millis(2000));
    println!("`main` function done")
}
