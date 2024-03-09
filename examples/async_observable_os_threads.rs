//! This `Observable` emits values and completes. It returns an empty `Subscription`,
//! making it unable to be unsubscribed from. Some operators like `take`, `switch_map`,
//! `merge_map`, `concat_map`, and `exhaust_map` require unsubscribe functionality to
//! work correctly.
//!
//! This asynchronous Observable utilizes an OS thread, preventing it from blocking
//! the current thread.
//!
//! To run this example, execute `cargo run --example async_observable_os_threads`.

use std::time::Duration;

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable,
};

fn main() {
    // Create a custom observable that emits values in a separate thread.
    let observable = Observable::new(|mut o| {
        // Launch a new thread for the Observable's processing and store its handle.
        let join_handle = std::thread::spawn(move || {
            for i in 0..=15 {
                // Emit the value to the subscriber.
                o.next(i);
                // Important. Put an await point after each emit or after some emits.
                // This allows the `take()` operator to function properly.
                // Not required in this example.
                std::thread::sleep(Duration::from_millis(1));
            }
            // Signal completion to the subscriber.
            o.complete();
        });

        // Return the subscription.
        Subscription::new(
            // In this example, we omit the unsubscribe functionality. Without it, we
            // can't unsubscribe, which prevents the `take()` operator, as well as
            // higher-order operators like `switch_map`, `merge_map`, `concat_map`,
            // and `exhaust_map`, from functioning as expected.
            UnsubscribeLogic::Nil,
            // Store the `JoinHandle` to enable waiting functionality using the
            // `Subscription` for this Observable thread to complete.
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
        .filter(|&v| v <= 10)
        .map(|v| format!("Mapped {}", v))
        .subscribe(observer);

    // Do something else here.
    println!("Print something while Observable is emitting.");

    // Because the subscription creates a new thread, we can utilize the `Subscription`
    // to wait for its completion. This ensures that the main thread won't terminate
    // prematurely and stop all child threads.
    if subscription.join().is_err() {
        // Handle error
    }

    println!("Custom Observable finished emmiting")
}
