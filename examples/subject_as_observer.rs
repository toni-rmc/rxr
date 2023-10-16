use std::time::Duration;

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subject, Subscribeable,
};

pub fn main() {
    // Make an Observable.
    let mut o = Observable::new(move |mut o: Subscriber<_>| {
        for i in 0..=10 {
            o.next(i);
            std::thread::sleep(Duration::from_millis(1));
        }
        o.complete();

        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    // Initialize a `Subject` and obtain its emitter and receiver.
    let (emitter, mut receiver) = Subject::emitter_receiver();

    // Register `Subscriber` 1.
    receiver.subscribe(Subscriber::new(
        |v| println!("Subscriber 1: {}", v),
        Some(|e| eprintln!("Error 1: {}", e)),
        Some(|| println!("Completed Subscriber 1")),
    ));

    // Register `Subscriber` 2.
    receiver
        // We're cloning the receiver so we can use it again.
        // Shallow clone: clones only the pointer to the `Subject`.
        .clone()
        .take(7) // For performance, prioritize placing `take()` as the first operator.
        .delay(1000)
        .map(|v| format!("mapped {}", v))
        .subscribe(Subscriber::new(
            |v| println!("Subscriber 2: {}", v),
            Some(|e| eprintln!("Error 2: {}", e)),
            Some(|| println!("Completed Subscriber 2")),
        ));

    // Register `Subscriber` 3.
    receiver
        .filter(|v| v % 2 == 0)
        .map(|v| format!("filtered {}", v))
        .subscribe(Subscriber::new(
            |v| println!("Subscriber 3: {}", v),
            Some(|e| eprintln!("Error 3: {}", e)),
            Some(|| println!("Completed Subscriber 3")),
        ));

    // Convert the emitter into an observer and subscribe it to the observable.
    o.subscribe(emitter.into());
}
