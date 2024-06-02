//! `Subject` example
//!
//! This example demonstrates the usage of the `Subject` in the `rxr` library.
//!
//! The `Subject` acts as both an observer and observable in reactive programming,
//! broadcasting values to multiple observers.
//!
//! To run this example, execute `cargo run --example subject`.

use std::fmt::Display;

use rxr::{subjects::Subject, subscribe::Subscriber};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber<T: Display>(subscriber_id: i32) -> Subscriber<T> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        |_| eprintln!("Error"),
        move || println!("Completed {}", subscriber_id),
    )
}

pub fn main() {
    // Initialize a `Subject` and obtain its emitter and receiver.
    let (mut emitter, mut receiver) = Subject::emitter_receiver();

    // Registers `Subscriber` 1.
    receiver.subscribe(create_subscriber(1));

    emitter.next(101); // Emits 101 to registered `Subscriber` 1.
    emitter.next(102); // Emits 102 to registered `Subscriber` 1.

    // All Observable operators can be applied to the receiver.
    // Registers mapped `Subscriber` 2.
    receiver
        .clone() // Shallow clone: clones only the pointer to the `Subject` object.
        .map(|v| format!("mapped {}", v))
        .subscribe(create_subscriber(2));

    // Registers `Subscriber` 3.
    receiver.subscribe(create_subscriber(3));

    // Emits 103 to registered `Subscriber`'s 1, 2 and 3.
    emitter.next(103);

    // Calls `complete` on registered `Subscriber`'s 1, 2 and 3.
    emitter.complete();

    // Subscriber 4: post-completion subscribe, completes immediately.
    receiver.subscribe(create_subscriber(4));

    // Called post-completion, does not emit.
    emitter.next(104);
}
