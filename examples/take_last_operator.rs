//! Demonstrating the `take_last()` operator
//!
//! This example showcases the functionality of the `take_last()` operator by
//! creating a stream and extracting the last `n` elements from it.
//!
//! To run this example, execute `cargo run --example take_last_operator`.

use std::sync::{Arc, Mutex};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable,
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

    // Capture and process only the last 8 values.
    let subscription = observable.take_last(8).subscribe(observer);

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for observable to finish before exiting the program.
    if subscription.join_concurrent().await.is_err() {
        // Handle error
    }

    println!("`main` function done")
}
