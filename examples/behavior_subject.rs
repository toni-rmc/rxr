//! `BehaviorSubject` example
//!
//! This example showcases the functionality of the `BehaviorSubject` in the `rxr` library.
//!
//! The `BehaviorSubject` is a type of subject in reactive programming that emits the
//! most recently emitted item and all subsequent items of the source observable to
//! its subscribers. It holds a current value, and new subscribers receive the last
//! emitted value immediately upon subscription. If no items have been emitted yet,
//! subscribers will receive a default or specified initial value.
//!
//! To run this example, execute `cargo run --example behavior_subject`.

use std::fmt::Display;

use rxr::{subjects::BehaviorSubject, subscribe::Subscriber};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber<T: Display>(subscriber_id: i32) -> Subscriber<T> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        |_| eprintln!("Error"),
        move || println!("Completed {}", subscriber_id),
    )
}

pub fn main() {
    // Initialize a `BehaviorSubject` with an initial value and obtain
    // its emitter and receiver.
    let (mut emitter, mut receiver) = BehaviorSubject::emitter_receiver(100);

    // Registers `Subscriber` 1 and emits the default value 100 to it.
    receiver.subscribe(create_subscriber(1));

    emitter.next(101); // Emits 101 to registered `Subscriber` 1.
    emitter.next(102); // Emits 102 to registered `Subscriber` 1.

    // All Observable operators can be applied to the receiver.
    // Registers mapped `Subscriber` 2 and emits (now the default) value 102 to it.
    receiver
        .clone() // Shallow clone: clones only the pointer to the `BehaviorSubject` object.
        .map(|v| format!("mapped {}", v))
        .subscribe(create_subscriber(2));

    // Registers `Subscriber` 3 and emits (now the default) value 102 to it.
    receiver.subscribe(create_subscriber(3));

    emitter.next(103); // Emits 103 to registered `Subscriber`'s 1, 2 and 3.

    emitter.complete(); // Calls `complete` on registered `Subscriber`'s 1, 2 and 3.

    // Subscriber 4: post-completion subscribe, completes immediately.
    receiver.subscribe(create_subscriber(4));

    // Called post-completion, does not emit.
    emitter.next(104);
}
