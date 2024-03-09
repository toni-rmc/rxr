//! This simple `Observable` emits values and completes. It returns an empty
//! `Subscription`, making it unable to be unsubscribed from. Some operators like
//! `take`, `switch_map`, `merge_map`, `concat_map`, and `exhaust_map` require
//! unsubscribe functionality to work correctly.
//!
//! Additionally, this is a synchronous Observable, so it blocks the current thread
//! until it completes emissions.
//!
//! To run this example, execute `cargo run --example basic_observable`.

use rxr::subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic};
use rxr::{Observable, Observer, Subscribeable};

fn main() {
    // Create a custom observable that emits values from 1 to 10.
    let mut emit_10_observable = Observable::new(|mut subscriber| {
        let mut i = 1;

        while i <= 10 {
            // Emit the value to the subscriber.
            subscriber.next(i);

            i += 1;
        }

        // Signal completion to the subscriber.
        subscriber.complete();

        // Return the empty subscription.
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    // Create the `Subscriber` with a mandatory `next` function, and optional
    // `complete` function. No need for `error` function in this simple example.
    let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
    observer.on_complete(|| println!("Completed"));

    // This observable does not use async or threads so it will block until it is done.
    // Observables are cold so if you comment out the line bellow nothing will be emitted.
    emit_10_observable.subscribe(observer);

    println!("Custom Observable finished emmiting")
}
