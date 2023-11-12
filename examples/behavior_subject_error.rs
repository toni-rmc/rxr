use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

use rxr::{subjects::BehaviorSubject, subscribe::Subscriber};
use rxr::{ObservableExt, Observer, Subscribeable};

pub fn create_subscriber(subscriber_id: i32) -> Subscriber<i32> {
    Subscriber::new(
        move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
        move |e| eprintln!("Error: {} {}", e, subscriber_id),
        || println!("Completed"),
    )
}

#[derive(Debug)]
struct BehaviorSubjectError(String);

impl Display for BehaviorSubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for BehaviorSubjectError {}

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
        .subscribe(Subscriber::new(
            move |v| println!("Subscriber #2 emitted: {}", v),
            |e| eprintln!("Error: {} 2", e),
            || println!("Completed"),
        ));

    // Registers `Subscriber` 3 and emits (now the default) value 102 to it.
    receiver.subscribe(create_subscriber(3));

    emitter.next(103); // Emits 103 to registered `Subscriber`'s 1, 2 and 3.

    // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
    emitter.error(Arc::new(BehaviorSubjectError(
        "BehaviorSubject error".to_string(),
    )));

    // Subscriber 4: subscribed after subject's error call; emits error and
    // does not emit further.
    receiver.subscribe(create_subscriber(4));

    emitter.next(104); // Called after subject's error call, does not emit.
}
