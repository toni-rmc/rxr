//! `AsyncSubject` example
//!
//! This example demonstrates the usage of the `AsyncSubject` in the `rxr` library.
//!
//! The `AsyncSubject` is a type of subject in reactive programming that emits only
//! the last value emitted by the source observable, only after that observable
//! completes. If the source observable terminates with an error, the `AsyncSubject`
//! will propagate that error.
//!
//! To run this example, execute `cargo run --example async_subject`.

use std::fmt::Display;

use rxr::{subjects::AsyncSubject, subscribe::Subscriber};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber<T: Display>(subscriber_id: i32) -> Subscriber<T> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        |_| eprintln!("Error"),
        move || println!("Completed {}", subscriber_id),
    )
}

pub fn main() {
    // Initialize a `AsyncSubject` and obtain its emitter and receiver.
    let (mut emitter, mut receiver) = AsyncSubject::emitter_receiver();

    // Registers `Subscriber` 1.
    receiver.subscribe(create_subscriber(1));

    emitter.next(101); // Stores 101 ast the latest value.
    emitter.next(102); // Latest value is now 102.

    // All Observable operators can be applied to the receiver.
    // Registers mapped `Subscriber` 2.
    receiver
        .clone() // Shallow clone: clones only the pointer to the `AsyncSubject` object.
        .map(|v| format!("mapped {}", v))
        .subscribe(create_subscriber(2));

    // Registers `Subscriber` 3.
    receiver.subscribe(create_subscriber(3));

    emitter.next(103); // Latest value is now 103.

    // Emits latest value (103) to registered `Subscriber`'s 1, 2 and 3 and calls
    // `complete` on each of them.
    emitter.complete();

    // Subscriber 4: post-completion subscribe, emits latest value (103) and completes.
    receiver.subscribe(create_subscriber(4));

    // Called post-completion, does not emit.
    emitter.next(104);
}
