//! Demonstrating the `take_until()` operator with a notifier observable
//!
//! This example illustrates the functionality of the `take_until()` operator by
//! creating a stream and emitting elements until a notifier observable emits a signal.
//!
//! To run this example, execute `cargo run --example take_until_operator`.

use std::sync::{Arc, Mutex};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subject, Subscribeable,
};
use tokio::{sync::mpsc::channel, task, time};

const UNSUBSCRIBE_SIGNAL: bool = true;

#[tokio::main]
async fn main() {
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
            for i in 0..100 {
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

    let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
    observer.on_complete(|| println!("Completed"));

    // Create notifier, it can be observable or one of the Subject variants.
    let (mut emitter, receiver) = Subject::<()>::emitter_receiver();

    // Turning Subject into an observable. To continue using the receiver later,
    // utilize `.clone()`, e.g. `receiver.clone().into()`.
    let subscription = observable
        .take_until(receiver.into(), false)
        .subscribe(observer);

    // Allowing some time for the `take_until` function to register the notifier
    // before emitting a signal. This step is unnecessary if you're not immediately
    // sending a signal.
    time::sleep(time::Duration::from_millis(1)).await;

    // Send signal to stop source observable emitting values.
    emitter.next(());

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for observable to finish before exiting the program.
    if subscription.join_concurrent().await.is_err() {
        // Handle error
    }

    println!("`main` function done")
}
