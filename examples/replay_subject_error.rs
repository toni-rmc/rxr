//! `ReplaySubject` example with error handling
//!
//! This example demonstrates the usage of the `ReplaySubject` emitting error
//! in the `rxr` library.
//!
//! The `ReplaySubject` is a type of subject in reactive programming that replays a
//! specified number of previously emitted items to new subscribers. It keeps a
//! buffer of past emissions and immediately provides these buffered items to any new
//! subscriber upon subscription. The number of items replayed is determined by the
//! specified buffer size.
//!
//! If no buffer size is specified, the `ReplaySubject` will replay all previously
//! emitted items to new subscribers.
//!
//! To run this example, execute `cargo run --example replay_subject_error`.

use std::{error::Error, fmt::Display, sync::Arc};

use rxr::{
    subjects::{BufSize, ReplaySubject},
    subscribe::Subscriber,
    Unsubscribeable,
};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber<T: Display>(subscriber_id: i32) -> Subscriber<T> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        move |e| eprintln!("Error: {} {}", e, subscriber_id),
        || println!("Completed"),
    )
}

#[derive(Debug)]
struct ReplaySubjectError(String);

impl Display for ReplaySubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for ReplaySubjectError {}

pub fn main() {
    // Initialize a `ReplaySubject` with an unbounded buffer size and obtain
    // its emitter and receiver.
    let (mut emitter, mut receiver) = ReplaySubject::emitter_receiver(BufSize::Unbounded);

    // Registers `Subscriber` 1.
    receiver.subscribe(create_subscriber(1));

    emitter.next(101); // Stores 101 and emits it to registered `Subscriber` 1.
    emitter.next(102); // Stores 102 and emits it to registered `Subscriber` 1.

    // All Observable operators can be applied to the receiver.
    // Registers mapped `Subscriber` 2 and emits buffered values (101, 102) to it.
    receiver
        .clone() // Shallow clone: clones only the pointer to the `ReplaySubject` object.
        .map(|v| format!("mapped {}", v))
        .subscribe(create_subscriber(2));

    // Registers `Subscriber` 3 and emits buffered values (101, 102) to it.
    receiver.subscribe(create_subscriber(3));

    // Stores 103 and emits it to registered `Subscriber`'s 1, 2 and 3.
    emitter.next(103);

    // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
    emitter.error(Arc::new(ReplaySubjectError(
        "ReplaySubject error".to_string(),
    )));

    // Subscriber 4: post-error subscribe, emits buffered values (101, 102, 103)
    // and emits error.
    receiver.subscribe(create_subscriber(4));

    // Called post-error, does not emit.
    emitter.next(104);

    // Closes receiver and clears registered subscribers.
    receiver.unsubscribe();
}
