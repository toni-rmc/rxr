# rxr

An implementation of the reactive extensions for the Rust programming language,
inspired by the popular RxJS library in JavaScript. Currently, **rxr** implements
a smaller subset of operators compared to RxJS.

- [API documentation](https://docs.rs/rxr)

## Design

**rxr** supports Observables and Subjects. You define your own Observables with
functionality you want. Your Observables can be synchronous or asynchronous. For
asynchronous Observables, you can utilize OS threads or Tokio tasks.<br/>
For examples on how to define your Observables see the [documentation].
To see what operators are currently implemented check the [ObservableExt] trait.

Note that you don't have to use Tokio in your projects to use **rxr** library.<br/>

[documentation]: https://docs.rs/rxr/latest/rxr/observable/struct.Observable.html
[ObservableExt]: https://docs.rs/rxr/latest/rxr/observable/trait.ObservableExt.html

## Examples

Asynchronous Observable with unsubscribe logic.

```rust
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic, Unsubscribeable},
    Observable, ObservableExt, Observer, Subscribeable,
};

const UNSUBSCRIBE_SIGNAL: bool = true;

fn main() {
    // Create a custom observable that emits values in a separate thread.
    let observable = Observable::new(|mut o| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn a new thread to await a signal sent from the unsubscribe logic.
        std::thread::spawn(move || {
            // Attempt to receive a signal sent from the unsubscribe logic.
            if let Ok(UNSUBSCRIBE_SIGNAL) = rx.recv() {
                // Update the `done_c` mutex with the received signal.
                *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
            }
        });

        // Launch a new thread for the Observable's processing and store its handle.
        let join_handle = std::thread::spawn(move || {
            for i in 0..=10000 {
                // If an unsubscribe signal is received, exit the loop and stop emissions.
                if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
                    break;
                }
                // Emit the value to the subscriber.
                o.next(i);
                // Important. Put an await point after each emit or after some emits.
                // This allows the `take()` operator to function properly.
                std::thread::sleep(Duration::from_millis(1));
            }
            // Signal completion to the subscriber.
            o.complete();
        });

        // Return a new `Subscription` with custom unsubscribe logic.
        Subscription::new(
            // The provided closure defines the behavior of the subscription when it
            // is unsubscribed. In this case, it sends a signal to an asynchronous
            // observable to stop emitting values.
            UnsubscribeLogic::Logic(Box::new(move || {
                if tx.send(UNSUBSCRIBE_SIGNAL).is_err() {
                    println!("Receiver dropped.");
                }
            })),
            // Store the `JoinHandle` for awaiting completion using the `Subscription`.
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
        // take utilizes our unsubscribe function to stop background emissions after
        // a specified item count.
        .take(500)
        .map(|v| format!("Mapped {}", v))
        .subscribe(observer);

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Unsubscribe from the observable to stop emissions.
    subscription.unsubscribe();

    // Allow some time for the main thread to confirm that the observable indeed
    // isn't emitting.
    std::thread::sleep(Duration::from_millis(2000));
    println!("`main` function done")
}
```

Utilizing a Subject as an Observer. This can be done with any variant of Subject.

```rust
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
```

Additional examples can be found in both the [`examples`] directory and the
[documentation].

[`examples`]: https://github.com/toni-rmc/rxr/tree/master/examples
[documentation]: https://docs.rs/rxr/latest/rxr/observable/struct.Observable.html

## Installation

Add a line into your Cargo.toml:

```toml
[dependencies]
rxr = "0.1.7"
```

## License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in rxr by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
</sub>
