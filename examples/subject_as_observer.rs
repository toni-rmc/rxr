//! Demonstrates how to use `Subject` emitter as an observer.
//!
//! To run this example, execute `cargo run --example subject_as_observer`.

use std::{fmt::Display, time::Duration};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subject, Subscribeable,
};

pub fn create_subscriber<T: Display>(subscriber_id: u32) -> Subscriber<T> {
    Subscriber::new(
        move |v: T| println!("Subscriber {}: {}", subscriber_id, v),
        move |e| eprintln!("Error {}: {}", subscriber_id, e),
        move || println!("Completed Subscriber {}", subscriber_id),
    )
}

pub fn main() {
    // Make an Observable.
    let mut observable = Observable::new(move |mut o: Subscriber<_>| {
        for i in 0..10 + 1 {
            o.next(i);
            std::thread::sleep(Duration::from_millis(1));
        }
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    // Initialize a `Subject` and obtain its emitter and receiver.
    let (emitter, mut receiver) = Subject::emitter_receiver();

    // Register `Subscriber` 1.
    receiver.subscribe(create_subscriber(1));

    // Register `Subscriber` 2.
    receiver
        // We're cloning the receiver so we can use it again.
        // Shallow clone: clones only the pointer to the `Subject`.
        .clone()
        .take(7) // For performance, prioritize placing `take()` as the first operator.
        .delay(1000)
        .map(|v| format!("mapped {}", v))
        .subscribe(create_subscriber(2));

    // Register `Subscriber` 3.
    receiver
        .filter(|v| v % 2 == 0)
        .map(|v| format!("filtered {}", v))
        .subscribe(create_subscriber(3));

    // Convert the emitter into an observer and subscribe it to the observable.
    observable.subscribe(emitter.into());
}
