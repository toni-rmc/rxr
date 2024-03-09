//! Demonstrating the `scan()` operator
//!
//! This example illustrates the functionality of the `scan()` operator, which
//! applies an accumulator function to each element emitted by an observable,
//! producing a new accumulated result on each emission.
//!
//! The `scan()` operator is useful for maintaining state across emissions and
//! performing cumulative transformations on the observable stream. It takes a
//! closure that defines the accumulation logic and an optional initial accumulator
//! value. The result is a new observable emitting the accumulated values.
//!
//! To run this example, execute `cargo run --example scan_operator`.

use std::sync::{Arc, Mutex};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable,
};
use tokio::{sync::mpsc::channel, task, time};

const UNSUBSCRIBE_SIGNAL: bool = true;

fn get_response_observable() -> Observable<&'static str> {
    Observable::new(|mut o| {
        task::spawn(async move {
            o.next("response");
            time::sleep(time::Duration::from_millis(1)).await;
            o.complete();
        });
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    })
}

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

    let mut observer = Subscriber::on_next(|v| println!("Emitted: {}", v));
    observer.on_complete(|| println!("Completed"));

    // Accumulate response strings into a single string.
    // The types of `total` and `n` may differ as long as `total` implements the `From<n>` trait.
    // In this example, `total` is of type `String`, and `n` is of type `&str`.
    let subscription = observable
        .take(6)
        .delay(100)
        .merge_map(|_| get_response_observable())
        .scan(|total, n| format!("{} {}", total, n), None)
        .subscribe(observer);

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for observable to finish before exiting the program.
    if subscription.join_concurrent().await.is_err() {
        // Handle error
    }

    println!("`main` function done")
}
