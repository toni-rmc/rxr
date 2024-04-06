//! Demonstrates the usage of the `connectable()` operator.
//!
//! This module contains examples showcasing how to use the `connectable()` operator
//! to convert a regular observable into a `Connectable` observable. It demonstrates
//! how to create a `Connectable` observable from different source observables and
//! how to use the `connect()` method to initiate emission of values.
//!
//! To run this example, execute `cargo run --example connectable_operator`.

use std::sync::{Arc, Mutex};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable, Unsubscribeable,
};
use tokio::{sync::mpsc::channel, task, time};

const UNSUBSCRIBE_SIGNAL: bool = true;

#[tokio::main]
async fn main() {
    // Make a source observable.
    let observable = Observable::new(|mut o| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        task::spawn(async move {
            if let Some(UNSUBSCRIBE_SIGNAL) = rx.recv().await {
                *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
            }
        });

        let join_handle = task::spawn(async move {
            for i in 0..10 + 1 {
                if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
                    break;
                }
                o.next(i);
                time::sleep(time::Duration::from_millis(1)).await;
            }
            o.complete();
        });

        Subscription::new(
            UnsubscribeLogic::Future(Box::pin(async move {
                if tx.send(UNSUBSCRIBE_SIGNAL).await.is_err() {
                    println!("Receiver dropped.");
                }
            })),
            SubscriptionHandle::JoinTask(join_handle),
        )
    });

    let mut observer1 = Subscriber::on_next(|v| println!("Observer 1 emitted {}", v));
    observer1.on_complete(|| println!("Observer 1 completed"));

    let mut observer2 = Subscriber::on_next(|v| println!("Observer 2 emitted {}", v));
    observer2.on_complete(|| println!("Observer 2 completed"));

    let mut observer3 = Subscriber::on_next(|v| println!("Observer 3 emitted {}", v));
    observer3.on_complete(|| println!("Observer 3 completed"));

    // Make a `Connectable` observable from the source observable.
    let mut connectable = observable.connectable();

    // Subscribe observers to `Connectable`.
    connectable.subscribe(observer1);
    connectable.subscribe(observer2);

    // You can also obtain a `Subscription` to unsubscribe individual observers
    // before calling `connect()`. In this case, `observer3` will be removed and will
    // not receive multicast emissions.
    let subscription3 = connectable.subscribe(observer3);
    subscription3.unsubscribe();

    // Connect `Connectable` to start emitting to all `Subscriber`'s.
    // No emissions happen if `connect()` is not called.
    let connected = connectable.connect();

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for `Connectable` observable to finish before exiting the program.
    // You can also use `connected.unsubscribe();` to stop all emissions.
    if connected.join_concurrent().await.is_err() {
        // Handle error
    }

    println!("`main` function done")
}
