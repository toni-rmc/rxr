//! `AsyncSubject` example with error handling
//!
//! This example demonstrates the usage of the `AsyncSubject` emitting error
//! in the `rxr` library.
//!
//! The `AsyncSubject` is a type of subject in reactive programming that emits only
//! the last value emitted by the source observable, only after that observable
//! completes. If the source observable terminates with an error, the `AsyncSubject`
//! will propagate that error.
//!
//! To run this example, execute `cargo run --example async_subject_error`.

use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

use rxr::{subjects::AsyncSubject, subscribe::Subscriber};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber(subscriber_id: i32) -> Subscriber<i32> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        move |e| eprintln!("Error: {} {}", e, subscriber_id),
        || println!("Completed"),
    )
}

#[derive(Debug)]
struct AsyncSubjectError(String);

impl Display for AsyncSubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for AsyncSubjectError {}

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
        .subscribe(Subscriber::new(
            move |v| println!("Subscriber #2 emitted: {}", v),
            |e| eprintln!("Error: {} 2", e),
            || println!("Completed"),
        ));

    // Registers `Subscriber` 3.
    receiver.subscribe(create_subscriber(3));

    emitter.next(103); // Latest value is now 103.

    // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
    emitter.error(Arc::new(AsyncSubjectError(
        "AsyncSubject error".to_string(),
    )));

    // Subscriber 4: subscribed after subject's error call; emits error and
    // does not emit further.
    receiver.subscribe(create_subscriber(4));

    emitter.next(104); // Called post-completion, does not emit.
}
