/**
 * This `Observable` emits values and completes, returning a `Subscription` that can
 * be unsubscribed from, enabling all operators to function correctly. It uses `Tokio`
 * tasks for asynchronous processing, preventing it from blocking the current thread.
 */
use std::sync::{Arc, Mutex};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable,
};

use tokio::{sync::mpsc::channel, task, time};

const UNSUBSCRIBE_SIGNAL: bool = true;

#[tokio::main()]
async fn main() {
    // Create a custom observable that emits values in a separate task.
    let observable = Observable::new(|mut o| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        // Spawn a new Tokio task to await a signal sent from the unsubscribe logic.
        task::spawn(async move {
            // Attempt to receive a signal sent from the unsubscribe logic.
            if let Some(UNSUBSCRIBE_SIGNAL) = rx.recv().await {
                // Update the `done_c` mutex with the received signal.
                *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
            }
        });

        // Launch a new Tokio task for the Observable's processing and store its handle.
        let join_handle = task::spawn(async move {
            for i in 0..=10000 {
                // If an unsubscribe signal is received, exit the loop and stop emissions.
                if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
                    break;
                }
                println!("----------- {}", i);
                // Emit the value to the subscriber.
                o.next(i);
                // Important. Put an await point after each emit or after some emits.
                // This allows the `take()` operator to function properly.
                time::sleep(time::Duration::from_millis(1)).await;
            }
            // Signal completion to the subscriber.
            o.complete();
        });

        // Return a new `Subscription` with custom unsubscribe logic.
        Subscription::new(
            // The provided closure defines the behavior of the subscription when it
            // is unsubscribed. In this case, it sends a signal to an asynchronous
            // observable to stop emitting values. If your closure requires Tokio
            // tasks or channels to send unsubscribe signals, consider using
            // `UnsubscribeLogic::Future`.
            UnsubscribeLogic::Future(Box::pin(async move {
                if tx.send(UNSUBSCRIBE_SIGNAL).await.is_err() {
                    println!("Receiver dropped.");
                }
            })),
            // Store the `JoinHandle` for awaiting completion using the `Subscription`.
            SubscriptionHandle::JoinTask(join_handle),
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
        .take(15)
        .map(|v| format!("Mapped {}", v))
        .delay(1000)
        .subscribe(observer);

    // Do something else here.
    println!("Do something while Observable is emitting.");

    // Wait for the subscription to either complete as a Tokio task or join an OS thread.
    if subscription.join_thread_or_task().await.is_err() {
        // Handle error
    }

    println!("`main` function done")
}
