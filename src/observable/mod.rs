#![allow(clippy::needless_doctest_main)]
//! The `observable` module provides the building blocks for creating and manipulating
//! observables, allowing for reactive programming in Rust.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{observer::Observer, subscribe::Fuse, subscription::subscribe::UnsubscribeLogic};
use crate::{
    subscribe::SubscriptionCollection,
    subscription::subscribe::{
        Subscribeable, Subscriber, Subscription, SubscriptionHandle, Unsubscribeable,
    },
};

enum Sender<T> {
    OSSender(std::sync::mpsc::Sender<T>),
    TokioSender(tokio::sync::mpsc::Sender<T>),
}

enum Receiver<T> {
    OSReceiver(std::sync::mpsc::Receiver<T>),
    TokioReceiver(tokio::sync::mpsc::Receiver<T>),
}

/// The `Observable` struct represents a source of values that can be observed
/// and transformed.
///
/// This struct serves as the foundation for creating, transforming, and working with
/// observables. It provides methods for applying operators, subscribing to emitted
/// values, and creating new observables.
///
/// # Example: basic synchronous `Observable`
///
/// This simple `Observable` emits values and completes. It returns an empty `Subscription`,
/// making it unable to be unsubscribed from. Some operators like `take`, `switch_map`,
/// `merge_map`, `concat_map`, and `exhaust_map` require unsubscribe functionality to
/// work correctly.
///
/// Additionally, this is a synchronous `Observable`, so it blocks the current thread
/// until it completes emission.
///
/// ```no_run
/// use rxr::subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic};
/// use rxr::{Observable, Observer, Subscribeable};
///
/// // Create a custom observable that emits values from 1 to 10.
/// let mut emit_10_observable = Observable::new(|mut subscriber| {
///     let mut i = 1;
///
///     while i <= 10 {
///         // Emit the value to the subscriber.
///         subscriber.next(i);
///         i += 1;
///     }
///     // Signal completion to the subscriber.
///     subscriber.complete();
///
///     // Return the empty subscription.
///     Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
/// });
///
/// // Create the `Subscriber` with a mandatory `next` function, and optional
/// // `complete` function. No need for `error` function in this simple example.
/// let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
/// observer.on_complete(|| println!("Completed"));
///
/// // This observable blocks until completion since it doesn't use async or
/// // threads. If you comment out the line below, no emissions will occur
/// // because observables are cold.
/// emit_10_observable.subscribe(observer);
///
/// println!("Custom Observable finished emmiting")
/// ```
///
/// # Example: basic asynchronous `Observable`
///
/// Emits values and completes, returning an empty `Subscription`, making it unable
/// to be unsubscribed from. Some operators like `take`, `switch_map`, `merge_map`,
/// `concat_map`, and `exhaust_map` require unsubscribe functionality to work correctly.
///
/// Utilizes an OS thread for asynchronous processing, preventing it from blocking
/// the current thread.
///
/// ```no_run
/// use std::time::Duration;
///
/// use rxr::{
///     subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
///     Observable, ObservableExt, Observer, Subscribeable,
/// };
///
/// // Create a custom observable that emits values from 1 to 10 in separate thread.
/// let observable = Observable::new(|mut o| {
///     // Launch a new thread for the Observable's processing and store its handle.
///     let join_handle = std::thread::spawn(move || {
///         for i in 0..=15 {
///             // Emit the value to the subscriber.
///             o.next(i);
///             // Important. Put an await point after each emit or after some emits.
///             // This allows the `take()` operator to function properly.
///             // Not required in this example.
///             std::thread::sleep(Duration::from_millis(1));
///         }
///         // Signal completion to the subscriber.
///         o.complete();
///     });
///
///     // Return the subscription.
///     Subscription::new(
///         // In this example, we omit the unsubscribe functionality. Without it, we
///         // can't unsubscribe, which prevents the `take()` operator, as well as
///         // higher-order operators like `switch_map`, `merge_map`, `concat_map`,
///         // and `exhaust_map`, from functioning as expected.
///         UnsubscribeLogic::Nil,
///         // Store the `JoinHandle` to enable waiting functionality using the
///         // `Subscription` for this Observable thread to complete.
///         SubscriptionHandle::JoinThread(join_handle),
///     )
/// });
///
/// // Create the `Subscriber` with a mandatory `next` function, and optional
/// // `complete` function. No need for `error` function in this simple example.
/// let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
/// observer.on_complete(|| println!("Completed"));
///
/// // This observable uses OS threads so it will not block the current thread.
/// // Observables are cold so if you comment out the statement bellow nothing
/// // will be emitted.
/// let subscription = observable
///     .filter(|&v| v <= 10)
///     .map(|v| format!("Mapped {}", v))
///     .subscribe(observer);
///
/// // Do something else here.
/// println!("Do something while Observable is emitting.");
///
/// // Because the subscription creates a new thread, we can utilize the `Subscription`
/// // to wait for its completion. This ensures that the main thread won't terminate
/// // prematurely and stop all child threads.
/// if subscription.join().is_err() {
///     // Handle error
/// }
///
/// println!("Custom Observable finished emmiting")
/// ```
///
/// # Example: asynchronous `Observable` with `unsubscribe`
///
/// Emits values and completes, returning a `Subscription` that can be unsubscribed
/// from, enabling all operators to function correctly. Utilizes an OS thread for
/// asynchronous processing, preventing it from blocking the current thread.
///
/// ```no_run
/// use std::{
///     sync::{Arc, Mutex},
///     time::Duration,
/// };
///
/// use rxr::{
///     subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic, Unsubscribeable},
///     Observable, ObservableExt, Observer, Subscribeable,
/// };
///
/// const UNSUBSCRIBE_SIGNAL: bool = true;
///
/// // Create a custom observable that emits values in a separate thread.
/// let observable = Observable::new(|mut o| {
///     let done = Arc::new(Mutex::new(false));
///     let done_c = Arc::clone(&done);
///     let (tx, rx) = std::sync::mpsc::channel();
///
///     // Spawn a new thread to await a signal sent from the unsubscribe logic.
///     std::thread::spawn(move || {
///         // Attempt to receive a signal sent from the unsubscribe logic.
///         if let Ok(UNSUBSCRIBE_SIGNAL) = rx.recv() {
///             // Update the `done_c` mutex with the received signal.
///             *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
///         }
///     });
///
///     // Launch a new thread for the Observable's processing and store its handle.
///     let join_handle = std::thread::spawn(move || {
///         for i in 0..=10000 {
///             // If an unsubscribe signal is received, exit the loop and stop emissions.
///             if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
///                 break;
///             }
///             // Emit the value to the subscriber.
///             o.next(i);
///             // Important. Put an await point after each emit or after some emits.
///             // This allows the `take()` operator to function properly.
///             std::thread::sleep(Duration::from_millis(1));
///         }
///         // Signal completion to the subscriber.
///         o.complete();
///     });
///
///     // Return a new `Subscription` with custom unsubscribe logic.
///     Subscription::new(
///         // The provided closure defines the behavior of the subscription when it
///         // is unsubscribed. In this case, it sends a signal to an asynchronous
///         // observable to stop emitting values.
///         UnsubscribeLogic::Logic(Box::new(move || {
///             if tx.send(UNSUBSCRIBE_SIGNAL).is_err() {
///                 println!("Receiver dropped.");
///             }
///         })),
///         // Store the `JoinHandle` for awaiting completion using the `Subscription`.
///         SubscriptionHandle::JoinThread(join_handle),
///     )
/// });
///
/// // Create the `Subscriber` with a mandatory `next` function, and optional
/// // `complete` function. No need for `error` function in this simple example.
/// let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
/// observer.on_complete(|| println!("Completed"));
///
/// // This observable uses OS threads so it will not block the current thread.
/// // Observables are cold so if you comment out the statement bellow nothing
/// // will be emitted.
/// let subscription = observable
///     // `take` utilizes our unsubscribe function to stop background emissions
///     // after a specified item count.
///     .take(500)
///     .map(|v| format!("Mapped {}", v))
///     .subscribe(observer);
///
/// // Do something else here.
/// println!("Do something while Observable is emitting.");
///
/// // Unsubscribe from the observable to stop emissions.
/// subscription.unsubscribe();
///
/// // Allow some time for the main thread to confirm that the observable indeed
/// // isn't emitting.
/// std::thread::sleep(Duration::from_millis(2000));
/// println!("`main` function done")
/// ```
///
/// # Example: asynchronous `Observable` with `Tokio`
///
/// Emits values and completes, returning a `Subscription` that can be unsubscribed
/// from, enabling all operators to function correctly. Utilizes `Tokio` tasks for
/// asynchronous processing, preventing it from blocking the current thread.
///
///```no_run
/// use std::sync::{Arc, Mutex};
///
/// use rxr::{
///     subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
///     Observable, ObservableExt, Observer, Subscribeable,
/// };
///
/// use tokio::{task, time, sync::mpsc::channel};
///
/// const UNSUBSCRIBE_SIGNAL: bool = true;
///
/// #[tokio::main()]
/// async fn main() {
///     // Create a custom observable that emits values in a separate task.
///     let observable = Observable::new(|mut o| {
///         let done = Arc::new(Mutex::new(false));
///         let done_c = Arc::clone(&done);
///         let (tx, mut rx) = channel(10);
///
///         // Spawn a new Tokio task to await a signal sent from the unsubscribe logic.
///         task::spawn(async move {
///             // Attempt to receive a signal sent from the unsubscribe logic.
///             if let Some(UNSUBSCRIBE_SIGNAL) = rx.recv().await {
///                 // Update the `done_c` mutex with the received signal.
///                 *done_c.lock().unwrap() = UNSUBSCRIBE_SIGNAL;
///             }
///         });
///
///         // Launch a new Tokio task for the Observable's processing and store its handle.
///         let join_handle = task::spawn(async move {
///             for i in 0..=10000 {
///                 // If an unsubscribe signal is received, exit the loop and stop emissions.
///                 if *done.lock().unwrap() == UNSUBSCRIBE_SIGNAL {
///                     break;
///                 }
///                 // Emit the value to the subscriber.
///                 o.next(i);
///                 // Important. Put an await point after each emit or after some emits.
///                 // This allows the `take()` operator to function properly.
///                 time::sleep(time::Duration::from_millis(1)).await;
///             }
///             // Signal completion to the subscriber.
///             o.complete();
///         });
///
///         // Return a new `Subscription` with custom unsubscribe logic.
///         Subscription::new(
///             // The provided closure defines the behavior of the subscription when it
///             // is unsubscribed. In this case, it sends a signal to an asynchronous
///             // observable to stop emitting values. If your closure requires Tokio
///             // tasks or channels to send unsubscribe signals, use `UnsubscribeLogic::Future`.
///             UnsubscribeLogic::Future(Box::pin(async move {
///                 if tx.send(UNSUBSCRIBE_SIGNAL).await.is_err() {
///                     println!("Receiver dropped.");
///                 }
///             })),
///             // Store the `JoinHandle` for awaiting completion using the `Subscription`.
///             SubscriptionHandle::JoinTask(join_handle),
///         )
///     });
///
///     // Create the `Subscriber` with a mandatory `next` function, and optional
///     // `complete` function. No need for `error` function in this simple example.
///     let mut observer = Subscriber::on_next(|v| println!("Emitted {}", v));
///     observer.on_complete(|| println!("Completed"));
///
///     // This observable uses Tokio tasks so it will not block the current thread.
///     // Observables are cold so if you comment out the statement bellow nothing
///     // will be emitted.
///     let subscription = observable
///         // `take` utilizes our unsubscribe function to stop background emissions
///         // after a specified item count.
///         .take(15)
///         .map(|v| format!("Mapped {}", v))
///         .delay(1000)
///         .subscribe(observer);
///
///     // Do something else here.
///     println!("Do something while Observable is emitting.");
///
///     // Wait for the subscription to either complete as a Tokio task or join an OS thread.
///     if subscription.join_concurrent().await.is_err() {
///         // Handle error
///     }
///
///     println!("`main` function done")
/// }
///```
///
/// # Example: `Observable` with error handling
///
/// Waits for user input and emits both a value and a completion signal upon success.
/// In case of any errors, it signals them to the attached `Observer`.
///
/// Ensure errors are wrapped in an `Arc` before passing them to the Observer's
/// `error` function.
///
///```no_run
/// use std::{error::Error, fmt::Display, io, sync::Arc};
///
/// use rxr::{subscribe::*, Observable, Observer, Subscribeable};
///
/// #[derive(Debug)]
/// struct MyErr(i32);
///
/// impl Display for MyErr {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "number should be less than 100, you entered {}", self.0)
///     }
/// }
///
/// impl Error for MyErr {}
///
/// // Creates an `Observable<i32>` that processes user input and emits or signals errors.
/// pub fn get_less_than_100() -> Observable<i32> {
///     Observable::new(|mut observer| {
///         let mut input = String::new();
///
///         println!("Please enter an integer (less than 100):");
///
///         if let Err(e) = io::stdin().read_line(&mut input) {
///             // Send input error to the observer.
///             observer.error(Arc::new(e));
///             return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
///         }
///
///         match input.trim().parse::<i32>() {
///             Err(e) => {
///                 // Send parsing error to the observer.
///                 observer.error(Arc::new(e));
///             }
///             Ok(num) if num > 100 => {
///                 // Send custom error to the observer.
///                 observer.error(Arc::new(MyErr(num)))
///             }
///             Ok(num) => {
///                 // Emit the parsed value to the observer.
///                 observer.next(num);
///             }
///         }
///
///         // Signal completion if there are no errors.
///         // Note: `complete` does not affect the outcome if `error` was called before it.
///         observer.complete();
///
///         Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
///     })
/// }
///
/// let observer = Subscriber::new(
///     |input| println!("You entered: {}", input),
///     |e| eprintln!("{}", e),
///     || println!("User input handled"),
/// );
///
/// let mut observable = get_less_than_100();
///
/// observable.subscribe(observer);
///```
pub struct Observable<T> {
    subscribe_fn: Box<dyn FnMut(Subscriber<T>) -> Subscription + Send + Sync>,
    fused: bool,
    defused: bool,
}

impl<T> Observable<T> {
    /// Creates a new `Observable` with the provided subscribe function.
    ///
    /// This method allows you to define custom behavior for the `Observable` by
    /// providing a subscribe function (`sf`), a closure that defines the behavior of
    /// the `Observable` when subscribed. When the `Observable` is subscribed to, the
    /// `sf` function is invoked to manage the delivery of values to the `Subscriber`.
    /// It should also return a `Subscription` that enables unsubscribing and can be
    /// used for awaiting `Tokio` tasks or joining OS threads when the `Observable`
    /// is asynchronous.
    pub fn new(sf: impl FnMut(Subscriber<T>) -> Subscription + Send + Sync + 'static) -> Self {
        Observable {
            subscribe_fn: Box::new(sf),
            fused: false,
            defused: false,
        }
    }

    /// Creates an empty observable.
    ///
    /// The resulting observable does not emit any values and immediately completes
    /// upon subscription. It serves as a placeholder or a base case for some
    /// observable operations.
    pub fn empty() -> Self {
        Observable {
            subscribe_fn: Box::new(|_| {
                Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
            }),
            fused: false,
            defused: false,
        }
    }

    /// Fuse the observable, allowing it to complete at most once.
    ///
    /// If `complete()` is called on a fused observable, any subsequent emissions
    /// will have no effect. This ensures that the observable is closed after the
    /// first completion call.
    ///
    /// By default, observables are not fused, allowing them to emit values even
    /// after calling `complete()` and permitting multiple calls to `complete()`.
    /// When an observable emits an error, it is considered closed and will no longer
    /// emit any further values, regardless of being fused or not.
    ///
    /// # Notes
    ///
    /// `fuse()` does not unsubscribe ongoing emissions from the observable;
    /// it simply ignores them after the first `complete()` call, ensuring that no
    /// more values are emitted.
    pub fn fuse(mut self) -> Self {
        self.fused = true;
        self.defused = false;
        self
    }

    /// Defuse the observable, allowing it to complete and emit values after calling
    /// `complete()`.
    ///
    /// Observables are defused by default, enabling them to emit values even after
    /// completion and allowing multiple calls to `complete()`. Calling `defuse()` is
    /// not necessary unless the observable has been previously fused using
    /// [`fuse()`](#method.fuse). Once an observable is defused, it can emit values
    /// and call `complete()` multiple times on its observers.
    ///
    /// # Notes
    ///
    /// Defusing an observable does not allow it to emit an error after the first
    /// error emission. Once an error is emitted, the observable is considered closed
    /// and will not emit any further values, regardless of being defused or not.
    pub fn defuse(mut self) -> Self {
        self.fused = false;
        self.defused = true;
        self
    }
}

/// The `ObservableExt` trait provides a set of extension methods that can be applied
/// to observables to transform and manipulate their behavior.
///
/// This trait enhances the capabilities of the `Observable` struct by allowing users
/// to chain operators together, creating powerful reactive pipelines.
pub trait ObservableExt<T: 'static>: Subscribeable<ObsType = T> {
    /// Transforms the items emitted by the observable using a transformation
    /// function.
    ///
    /// The transformation function `f` is applied to each item emitted by the
    /// observable, and the resulting value is emitted by the resulting observable.
    fn map<U, F>(mut self, f: F) -> Observable<U>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnOnce(T) -> U) + Copy + Sync + Send + 'static,
        U: 'static,
    {
        let (fused, defused) = self.get_fused();
        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut u = Subscriber::new(
                move |v| {
                    let t = f(v);
                    o_shared.lock().unwrap().next(t);
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Filters the items emitted by the observable based on a predicate function.
    ///
    /// Only items for which the predicate function returns `true` will be emitted
    /// by the resulting observable.
    fn filter<P>(mut self, predicate: P) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
        P: (FnOnce(&T) -> bool) + Copy + Sync + Send + 'static,
    {
        let (fused, defused) = self.get_fused();
        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut u = Subscriber::new(
                move |v| {
                    if predicate(&v) {
                        o_shared.lock().unwrap().next(v);
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Skips the first `n` items emitted by the observable and then emits the rest.
    ///
    /// If `n` is greater than or equal to the total number of items, it behaves as
    /// if the observable is complete and emits no items.
    fn skip(mut self, n: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        let (fused, defused) = self.get_fused();
        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut n = n;
            let mut u = Subscriber::new(
                move |v| {
                    if n > 0 {
                        n -= 1;
                        return;
                    }
                    o_shared.lock().unwrap().next(v);
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Delays the emissions from the observable by the specified number of
    /// milliseconds.
    ///
    /// The `delay` operator introduces a time delay for emissions from the
    /// observable, determined by the specified duration.
    fn delay(mut self, num_of_ms: u64) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        let (fused, defused) = self.get_fused();
        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut u = Subscriber::new(
                move |v| {
                    std::thread::sleep(Duration::from_millis(num_of_ms));
                    o_shared.lock().unwrap().next(v);
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Emits at most the first `n` items emitted by the observable, then
    /// unsubscribes.
    ///
    /// If the observable emits more than `n` items, this operator will only allow
    /// the first `n` items to be emitted. After emitting `n` items, it will
    /// unsubscribe from the observable.
    ///
    /// # Notes
    ///
    /// For `Subject` variants, using `take(n)` as the initial operator
    /// (e.g., `subject.take(n).delay(n)`) will not call unsubscribe and remove
    /// registered subscribers for performance reasons.
    ///
    /// However, when used as a non-initial operator (e.g., `subject.delay(n).take(n)`),
    /// it will call unsubscribe and remove registered subscribers.
    fn take(mut self, n: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        let (fused, defused) = self.get_fused();
        let mut i = 0;

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut use_tokio = false;
            let mut is_current = false;
            let tx;
            let rx;

            // Check if Tokio runtime is used.
            let tokio_handle = tokio::runtime::Handle::try_current();
            if let Ok(t) = &tokio_handle {
                // If it is check if runtime flavor is `current_thread`.
                if let tokio::runtime::RuntimeFlavor::CurrentThread = t.runtime_flavor() {
                    is_current = true;
                }
            }
            // Open non-blocking channel only if Tokio is used and runtime flavor is
            // not `current_thread.`
            if let (Ok(_), false) = (&tokio_handle, is_current) {
                let (sender, receiver) = tokio::sync::mpsc::channel(10);
                tx = Sender::TokioSender(sender);
                rx = Receiver::TokioReceiver(receiver);
                use_tokio = true;
            } else {
                // Tokio runtime flavor is `current_thread` or Tokio is not used at
                // all; open `std` sync channel.
                let (sender, receiver) = std::sync::mpsc::channel();
                tx = Sender::OSSender(sender);
                rx = Receiver::OSReceiver(receiver);
            }
            let mut signal_sent = false;

            // Alternative implementation for Subject's if desired behavior is to
            // skip call to unsubscribe() when take() operator is used. This might be
            // used for performance reasons because opening a channel and spawning a
            // new thread can be skipped when this operator is used on Subject's.
            if self.is_subject() {
                signal_sent = true;
            }

            let mut u = Subscriber::new(
                move |v| {
                    if i < n {
                        i += 1;
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        signal_sent = true;
                        // Check which channel is used and use its sender to send
                        // unsubscribe signal.
                        if use_tokio {
                            if let Ok(h) = &tokio_handle {
                                if let Sender::TokioSender(s) = &tx {
                                    let s = s.clone();
                                    h.spawn(async move {
                                        s.send(true).await.unwrap();
                                    });
                                }
                            }
                        } else if let Sender::OSSender(s) = &tx {
                            let _ = s.send(true);
                        }
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = true;
            self.set_fused(fused, defused);

            let mut unsubscriber = self.subscribe(u);
            let ijh = unsubscriber.subscription_future;
            unsubscriber.subscription_future = SubscriptionHandle::Nil;

            if self.is_subject() {
                // Skip opening a thread or a task if this method is called
                // on one of the Subject's.
                return Subscription::new(
                    UnsubscribeLogic::Logic(Box::new(move || {
                        unsubscriber.unsubscribe();
                    })),
                    ijh,
                );
            }
            let mut is_future = false;
            if let UnsubscribeLogic::Future(_) = &unsubscriber.unsubscribe_logic {
                is_future = true;
            };
            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            // Check which channel is used and use its receiver to receive
            // unsubscribe signal.
            match rx {
                Receiver::TokioReceiver(mut receiver) => {
                    tokio::task::spawn(async move {
                        if receiver.recv().await.is_some() {
                            // println!("SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
                Receiver::OSReceiver(receiver) => {
                    std::thread::spawn(move || {
                        if receiver.recv().is_ok() {
                            // println!("---- SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
            };

            if is_future {
                return Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        if let Some(s) = u_cloned.lock().unwrap().take() {
                            s.unsubscribe();
                        }
                    })),
                    ijh,
                );
            }

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                ijh,
            )
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Continuously emits the values from the source observable until an event occurs,
    /// triggered by an emitted value from a separate `notifier` observable.
    ///
    /// The `takeUntil` operator subscribes to and starts replicating the behavior of
    /// the source observable. Simultaneously, it observes a second observable,
    /// referred to as the `notifier`, provided by the user. When the `notifier` emits
    /// a value, the resulting observable stops replicating the source observable and
    /// completes. If the `notifier` completes without emitting any value, `takeUntil`
    /// will pass all values from the source observable. When the `notifier` triggers
    /// its first emission `take_until` unsubscribes from ongoing emissions of the
    /// source observable.
    ///
    /// The `take_until` operator accepts a second parameter, `unsubscribe_notifier`,
    /// allowing control over whether `takeUntil` will attempt to unsubscribe from
    /// emissions of the `notifier` observable. When set to `true`, `takeUntil`
    /// actively attempts to unsubscribe from the `notifier`'s emissions. When set to
    /// `false`, `takeUntil` refrains from attempting to unsubscribe from the
    /// `notifier`, allowing the emissions to continue unaffected.
    fn take_until<U: 'static>(
        mut self,
        notifier: Observable<U>,
        unsubscribe_notifier: bool,
    ) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        let notifier = Arc::new(Mutex::new(notifier));
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut use_tokio = false;
            let mut is_current = false;
            let tx;
            let rx;

            // Check if Tokio runtime is used.
            let tokio_handle = tokio::runtime::Handle::try_current();
            if let Ok(t) = &tokio_handle {
                // If it is check if runtime flavor is `current_thread`.
                if let tokio::runtime::RuntimeFlavor::CurrentThread = t.runtime_flavor() {
                    is_current = true;
                }
            }
            // Open non-blocking channel only if Tokio is used and runtime flavor is
            // not `current_thread.`
            if let (Ok(_), false) = (&tokio_handle, is_current) {
                let (sender, receiver) = tokio::sync::mpsc::channel(10);
                tx = Sender::TokioSender(sender);
                rx = Receiver::TokioReceiver(receiver);
                use_tokio = true;
            } else {
                // Tokio runtime flavor is `current_thread` or Tokio is not used at
                // all; open `std` sync channel.
                let (sender, receiver) = std::sync::mpsc::channel();
                tx = Sender::OSSender(sender);
                rx = Receiver::OSReceiver(receiver);
            }
            let mut signal_sent = false;

            let notifier_next_called = Arc::new(Mutex::new(false));
            let notifier_next_called_cl = Arc::clone(&notifier_next_called);

            // Make an inner Observer which will set `notifier called` flag.
            let observer =
                Subscriber::on_next(move |_: U| *notifier_next_called_cl.lock().unwrap() = true);

            let notifier = Arc::clone(&notifier);
            let subscription = Arc::new(Mutex::new(None));
            let subscription_cl = Arc::clone(&subscription);

            if use_tokio || is_current {
                tokio::task::spawn(async move {
                    let subscription = notifier.lock().unwrap().subscribe(observer);
                    *subscription_cl.lock().unwrap() = Some(subscription);
                });
            } else {
                std::thread::spawn(move || {
                    let subscription = notifier.lock().unwrap().subscribe(observer);
                    *subscription_cl.lock().unwrap() = Some(subscription);
                });
            }

            // Alternative implementation for Subject's if desired behavior is to
            // skip call to unsubscribe() when take() operator is used. This might be
            // used for performance reasons because opening a channel and spawning a
            // new thread can be skipped when this operator is used on Subject's.
            if self.is_subject() {
                signal_sent = true;
            }

            let mut u = Subscriber::new(
                move |v| {
                    if !(*notifier_next_called.lock().unwrap()) {
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        signal_sent = true;
                        // Check which channel is used and use its sender to send
                        // unsubscribe signal.
                        if use_tokio {
                            if let Ok(h) = &tokio_handle {
                                if let Sender::TokioSender(s) = &tx {
                                    let s = s.clone();
                                    h.spawn(async move {
                                        s.send(true).await.unwrap();
                                    });
                                }
                            }
                        } else if let Sender::OSSender(s) = &tx {
                            let _ = s.send(true);
                        }
                        if unsubscribe_notifier {
                            if let Some(s) = subscription.lock().unwrap().take() {
                                s.unsubscribe();
                            }
                        }
                    } else if unsubscribe_notifier {
                        if let Some(s) = subscription.lock().unwrap().take() {
                            s.unsubscribe();
                        }
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = true;
            self.set_fused(fused, defused);

            let mut unsubscriber = self.subscribe(u);
            let ijh = unsubscriber.subscription_future;
            unsubscriber.subscription_future = SubscriptionHandle::Nil;

            if self.is_subject() {
                // Skip opening a thread or a task if this method is called
                // on one of the Subject's.
                return Subscription::new(
                    UnsubscribeLogic::Logic(Box::new(move || {
                        unsubscriber.unsubscribe();
                    })),
                    ijh,
                );
            }
            let mut is_future = false;
            if let UnsubscribeLogic::Future(_) = &unsubscriber.unsubscribe_logic {
                is_future = true;
            };
            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            // Check which channel is used and use its receiver to receive
            // unsubscribe signal.
            match rx {
                Receiver::TokioReceiver(mut receiver) => {
                    tokio::task::spawn(async move {
                        if receiver.recv().await.is_some() {
                            // println!("SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE Tokio called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
                Receiver::OSReceiver(receiver) => {
                    std::thread::spawn(move || {
                        if receiver.recv().is_ok() {
                            // println!("---- SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
            };

            if is_future {
                return Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        if let Some(s) = u_cloned.lock().unwrap().take() {
                            s.unsubscribe();
                        }
                    })),
                    ijh,
                );
            }

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                ijh,
            )
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Continues emitting values from the source observable as long as each value
    /// meets the specified `predicate` criteria. The operation concludes immediately
    /// upon encountering the first value that doesn't satisfy the `predicate`.
    ///
    /// Upon subscription, `takeWhile` starts replicating the source observable.
    /// Every emitted value from the source is evaluated by the `predicate` function,
    /// returning a boolean that represents a condition for the source values. The
    /// resulting observable continues emitting source values until the `predicate`
    /// yields `false`. When the specified condition is no longer met, `takeWhile`
    /// ceases mirroring the source, subsequently unsubscribing from the source to
    /// stop background emissions.
    fn take_while<P>(mut self, predicate: P) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
        P: (FnOnce(&T) -> bool) + Copy + Sync + Send + 'static,
    {
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut use_tokio = false;
            let mut is_current = false;
            let tx;
            let rx;

            // Check if Tokio runtime is used.
            let tokio_handle = tokio::runtime::Handle::try_current();
            if let Ok(t) = &tokio_handle {
                // If it is check if runtime flavor is `current_thread`.
                if let tokio::runtime::RuntimeFlavor::CurrentThread = t.runtime_flavor() {
                    is_current = true;
                }
            }
            // Open non-blocking channel only if Tokio is used and runtime flavor is
            // not `current_thread.`
            if let (Ok(_), false) = (&tokio_handle, is_current) {
                let (sender, receiver) = tokio::sync::mpsc::channel(10);
                tx = Sender::TokioSender(sender);
                rx = Receiver::TokioReceiver(receiver);
                use_tokio = true;
            } else {
                // Tokio runtime flavor is `current_thread` or Tokio is not used at
                // all; open `std` sync channel.
                let (sender, receiver) = std::sync::mpsc::channel();
                tx = Sender::OSSender(sender);
                rx = Receiver::OSReceiver(receiver);
            }
            let mut signal_sent = false;

            // Alternative implementation for Subject's if desired behavior is to
            // skip call to unsubscribe() when take() operator is used. This might be
            // used for performance reasons because opening a channel and spawning a
            // new thread can be skipped when this operator is used on Subject's.
            if self.is_subject() {
                signal_sent = true;
            }

            let mut u = Subscriber::new(
                move |v| {
                    if predicate(&v) {
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        signal_sent = true;
                        // Check which channel is used and use its sender to send
                        // unsubscribe signal.
                        if use_tokio {
                            if let Ok(h) = &tokio_handle {
                                if let Sender::TokioSender(s) = &tx {
                                    let s = s.clone();
                                    h.spawn(async move {
                                        s.send(true).await.unwrap();
                                    });
                                }
                            }
                        } else if let Sender::OSSender(s) = &tx {
                            let _ = s.send(true);
                        }
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = true;
            self.set_fused(fused, defused);

            let mut unsubscriber = self.subscribe(u);
            let ijh = unsubscriber.subscription_future;
            unsubscriber.subscription_future = SubscriptionHandle::Nil;

            if self.is_subject() {
                // Skip opening a thread or a task if this method is called
                // on one of the Subject's.
                return Subscription::new(
                    UnsubscribeLogic::Logic(Box::new(move || {
                        unsubscriber.unsubscribe();
                    })),
                    ijh,
                );
            }
            let mut is_future = false;
            if let UnsubscribeLogic::Future(_) = &unsubscriber.unsubscribe_logic {
                is_future = true;
            };
            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            // Check which channel is used and use its receiver to receive
            // unsubscribe signal.
            match rx {
                Receiver::TokioReceiver(mut receiver) => {
                    tokio::task::spawn(async move {
                        if receiver.recv().await.is_some() {
                            // println!("SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
                Receiver::OSReceiver(receiver) => {
                    std::thread::spawn(move || {
                        if receiver.recv().is_ok() {
                            // println!("---- SIGNAL received");
                            if let Some(s) = unsubscriber.lock().unwrap().take() {
                                // println!("UNSUBSCRIBE called");
                                s.unsubscribe();
                            }
                        }
                    });
                }
            };

            if is_future {
                return Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        if let Some(s) = u_cloned.lock().unwrap().take() {
                            s.unsubscribe();
                        }
                    })),
                    ijh,
                );
            }

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                ijh,
            )
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Produces an observable that emits, at maximum, the final `count` values
    /// emitted by the source observable.
    ///
    /// Utilizing `takeLast` creates an observable that retains up to 'count' values
    /// in memory until the source observable completes. Upon completion, it delivers
    /// all stored values in their original order to the consumer and signals completion.
    ///
    /// In scenarios where the source completes before reaching the specified `count`
    /// in `takeLast`, it emits all received values up to that point and then signals completion.
    ///
    /// # Notes
    ///
    /// When applied to an observable that never completes, `takeLast` yields an
    /// observable that doesn't emit any value.
    fn take_last(mut self, count: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
        T: Send,
    {
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let last_values_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(count)));
            let last_values_buffer_cl = Arc::clone(&last_values_buffer);

            let fused = o.fused;
            let defused = o.defused;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let i = Arc::new(Mutex::new(0));
            let i_cl = Arc::clone(&i);

            let mut u = Subscriber::new(
                move |v| {
                    if count == 0 {
                        return;
                    }
                    if let (Ok(mut counter), Ok(mut last_values_buffer)) =
                        (i.lock(), last_values_buffer_cl.lock())
                    {
                        *counter += 1;
                        if *counter > count {
                            last_values_buffer.pop_front();
                        }
                        last_values_buffer.push_back(v);
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    if let (Ok(mut o), Ok(mut last_values_buffer)) =
                        (o_shared.lock(), last_values_buffer.lock())
                    {
                        while let Some(item) = last_values_buffer.pop_front() {
                            o.next(item);
                        }
                        let _ = i_cl.lock().map(|mut counter| *counter = 0);
                        o.complete();
                    }
                },
            );
            u.take_wrapped = true;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Merges the current observable with a vector of observables, emitting items
    /// from all of them concurrently.
    fn merge(mut self, mut sources: Vec<Observable<T>>) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        fn wrap_subscriber<S: 'static>(
            s: Arc<Mutex<Subscriber<S>>>,
            is_fused: bool,
            is_defused: bool,
            is_take_wrapped: bool,
        ) -> Subscriber<S> {
            let s_complete = s.clone();
            let s_error = s.clone();

            let mut s = Subscriber::new(
                move |v| {
                    s.lock().unwrap().next(v);
                },
                move |e| {
                    s_error.lock().unwrap().error(e);
                },
                move || {
                    s_complete.lock().unwrap().complete();
                },
            );
            s.take_wrapped = is_take_wrapped;
            s.set_fused(is_fused, is_defused);
            s
        }

        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let take_wrapped = o.take_wrapped;
            let fused = o.fused;
            let defused = o.defused;

            // o.fused = false;
            // o.defused = false;
            let o = Arc::new(Mutex::new(o));
            let mut subscriptions = Vec::with_capacity(sources.len());

            let mut use_tokio_task = false;
            self.set_fused(fused, defused);
            let s = self.subscribe(wrap_subscriber(o.clone(), fused, defused, take_wrapped));

            if let UnsubscribeLogic::Future(_) = &s.unsubscribe_logic {
                use_tokio_task = true;
            }

            for source in &mut sources {
                let wrapped = wrap_subscriber(o.clone(), fused, defused, false);
                // source.set_fused((fused, defused));
                let subscription = source.subscribe(wrapped);

                if let UnsubscribeLogic::Future(_) = &subscription.unsubscribe_logic {
                    use_tokio_task = true;
                }
                subscriptions.push(subscription);
            }

            // If Tokio is used with `current_thread` runtime flavor returned
            // Subscriber will be `UnsubscribeLogic::Logic` so that `take()` can
            // unsubscribe background emissions in all cases.
            if let Ok(handle) = s.runtime_handle.as_ref() {
                if let tokio::runtime::RuntimeFlavor::CurrentThread = handle.runtime_flavor() {
                    use_tokio_task = false;
                }
            }
            subscriptions.push(s);

            let subscriptions = Arc::new(Mutex::new(Some(subscriptions)));
            let sc = Arc::clone(&subscriptions);

            if use_tokio_task {
                return Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        let subscriptions = subscriptions.lock().unwrap().take();

                        if let Some(subscriptions) = subscriptions {
                            for subscription in subscriptions {
                                subscription.unsubscribe();
                            }
                        }
                    })),
                    SubscriptionHandle::JoinSubscriptions(SubscriptionCollection::new(sc, true)),
                );
            }
            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    let subscriptions = subscriptions.lock().unwrap().take();

                    if let Some(subscriptions) = subscriptions {
                        for subscription in subscriptions {
                            subscription.unsubscribe();
                        }
                    }
                })),
                SubscriptionHandle::JoinSubscriptions(SubscriptionCollection::new(sc, false)),
            )
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Merges the current observable with another observable, emitting items from
    /// both concurrently.
    fn merge_one(mut self, mut source: Observable<T>) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        fn wrap_subscriber<S: 'static>(
            s: Arc<Mutex<Subscriber<S>>>,
            is_fused: bool,
            is_defused: bool,
            is_take_wrapped: bool,
        ) -> Subscriber<S> {
            let s_complete = s.clone();
            let s_error = s.clone();

            let mut s = Subscriber::new(
                move |v| {
                    s.lock().unwrap().next(v);
                },
                move |e| {
                    s_error.lock().unwrap().error(e);
                },
                move || {
                    s_complete.lock().unwrap().complete();
                },
            );
            s.take_wrapped = is_take_wrapped;
            s.set_fused(is_fused, is_defused);
            s
        }

        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let take_wrapped = o.take_wrapped;
            let fused = o.fused;
            let defused = o.defused;

            // o.fused = false;
            // o.defused = false;
            let o = Arc::new(Mutex::new(o));

            let wrapped = wrap_subscriber(o.clone(), fused, defused, take_wrapped);
            let wrapped2 = wrap_subscriber(o, fused, defused, false);

            let mut use_tokio_task = false;

            self.set_fused(fused, defused);
            let s1 = self.subscribe(wrapped);
            let s2 = source.subscribe(wrapped2);

            match (&s1.unsubscribe_logic, &s2.unsubscribe_logic) {
                (UnsubscribeLogic::Future(_), _) => use_tokio_task = true,
                (_, UnsubscribeLogic::Future(_)) => use_tokio_task = true,
                _ => (),
            }

            // If Tokio is used with `current_thread` runtime flavor returned
            // Subscriber will be `UnsubscribeLogic::Logic` so that `take()` can
            // unsubscribe background emissions in all cases.
            if let Ok(handle) = s1.runtime_handle.as_ref() {
                if let tokio::runtime::RuntimeFlavor::CurrentThread = handle.runtime_flavor() {
                    use_tokio_task = false;
                }
            }
            let subscriptions = vec![s1, s2];
            let subscriptions = Arc::new(Mutex::new(Some(subscriptions)));
            let sc = Arc::clone(&subscriptions);

            if use_tokio_task {
                return Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        let subscriptions = subscriptions.lock().unwrap().take();

                        if let Some(subscriptions) = subscriptions {
                            for subscription in subscriptions {
                                subscription.unsubscribe();
                            }
                        }
                    })),
                    SubscriptionHandle::JoinSubscriptions(SubscriptionCollection::new(sc, true)),
                );
            }
            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    let subscriptions = subscriptions.lock().unwrap().take();

                    if let Some(subscriptions) = subscriptions {
                        for subscription in subscriptions {
                            subscription.unsubscribe();
                        }
                    }
                })),
                SubscriptionHandle::JoinSubscriptions(SubscriptionCollection::new(sc, false)),
            )
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Transforms the items emitted by an observable into observables, and flattens
    /// the emissions into a single observable, ignoring previous emissions once a
    /// new one is encountered. This is similar to `map`, but switches to a new inner
    /// observable whenever a new item is emitted.
    ///
    /// # Parameters
    /// - `project`: A closure that maps each source item to an observable.
    ///   The closure returns the observable for each item, and the emissions from
    ///   these observables are flattened into a single observable.
    ///
    /// # Returns
    /// An observable that emits the items from the most recently emitted inner
    /// observable.
    fn switch_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let mut current_subscription: Option<Subscription> = None;

            let mut u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        },
                        move || {
                            o_cloned_c.lock().unwrap().complete();
                        },
                    );

                    if let Some(subscription) = current_subscription.take() {
                        subscription.unsubscribe();
                    }

                    let s = inner_observable.subscribe(inner_subscriber);
                    current_subscription = Some(s);
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Transforms the items emitted by the source observable into other observables,
    /// and merges them into a single observable stream.
    ///
    /// This operator applies the provided `project` function to each item emitted by
    /// the source observable. The function returns another observable. The operator
    /// subscribes to these inner observables concurrently and merges their emissions
    /// into one observable stream.
    ///
    /// # Parameters
    ///
    /// - `project`: A closure that maps each item emitted by the source observable
    ///   to another observable.
    ///
    /// # Returns
    ///
    /// An observable that emits items merged from the inner observables produced by
    /// the `project` function.
    fn merge_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let mut u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        },
                        move || {
                            o_cloned_c.lock().unwrap().complete();
                        },
                    );
                    inner_observable.subscribe(inner_subscriber);
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Transforms the items emitted by the source observable into other observables
    /// using a closure, and concatenates them into a single observable stream,
    /// waiting for each inner observable to complete before moving to the next.
    ///
    /// This operator applies the provided `project` function to each item emitted by
    /// the source observable. The function returns another observable. The operator
    /// subscribes to these inner observables sequentially and concatenates their
    /// emissions into one observable stream.
    ///
    /// # Parameters
    ///
    /// - `project`: A closure that maps each item emitted by the source observable
    ///   to another observable.
    ///
    /// # Returns
    ///
    /// An observable that emits items concatenated from the inner observables
    /// produced by the `project` function.
    fn concat_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let pending_observables: Arc<Mutex<VecDeque<(Observable<R>, Subscriber<R>)>>> =
                Arc::new(Mutex::new(VecDeque::new()));

            let mut first_pass = true;

            let mut u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let po_cloned = Arc::clone(&pending_observables);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        },
                        move || {
                            o_cloned_c.lock().unwrap().complete();
                            if let Some((mut io, is)) = po_cloned.lock().unwrap().pop_front() {
                                io.subscribe(is);
                            }
                        },
                    );

                    if first_pass {
                        inner_observable.subscribe(inner_subscriber);
                        first_pass = false;
                        return;
                    }
                    pending_observables
                        .lock()
                        .unwrap()
                        .push_back((inner_observable, inner_subscriber));
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }

    /// Maps each item emitted by the source observable to an inner observable using
    /// a closure. It subscribes to these inner observables, ignoring new items until
    /// the current inner observable completes.
    ///
    /// # Parameters
    ///
    /// - `project`: A closure that maps each item to an inner observable.
    ///
    /// # Returns
    ///
    /// An observable that emits inner observables exclusively. Inner observables do
    /// not emit and remain ignored if a preceding inner observable is still emitting.
    /// The emission of a subsequent inner observable is allowed only after the
    /// current one completes its emission.
    fn exhaust_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        let (fused, defused) = self.get_fused();

        let mut observable = Observable::new(move |o| {
            let fused = o.fused;
            let defused = o.defused;
            let take_wrapped = o.take_wrapped;

            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let active_subscription = Arc::new(Mutex::new(false));
            let guard = Arc::new(Mutex::new(true));

            let mut u = Subscriber::new(
                move |v| {
                    let as_cloned = Arc::clone(&active_subscription);
                    let as_cloned2 = Arc::clone(&active_subscription);
                    let project = Arc::clone(&project);

                    let _guard = guard.lock().unwrap();

                    // Check if previous subscription completed.
                    let is_previous_subscription_active = *as_cloned.lock().unwrap();

                    // if hn {
                    //     println!("TRY TO SEND ??????????????????????????????");
                    //     return;
                    // }
                    // else {
                    //     println!("SENT!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                    //     *as_cloned.lock().unwrap() = true;
                    // }

                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        },
                        move || {
                            o_cloned_c.lock().unwrap().complete();

                            // Mark this inner subscription as completed so that next
                            // one can be allowed to emit all of its values.
                            *as_cloned2.lock().unwrap() = false;
                        },
                    );

                    // TODO: move this check at the top of the function and return early.
                    // Do not subscribe current inner subscription if previous one is active.
                    if !is_previous_subscription_active {
                        // tokio::task::spawn(async move {
                        // Mark this inner subscription as active so other following
                        // subscriptions are rejected until this one completes.
                        *as_cloned.lock().unwrap() = true;
                        inner_observable.subscribe(inner_subscriber);
                        // });
                    }
                },
                move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                },
                move || {
                    o_cloned_c.lock().unwrap().complete();
                },
            );
            u.take_wrapped = take_wrapped;
            self.set_fused(fused, defused);
            self.subscribe(u)
        });
        observable.set_fused(fused, defused);
        observable
    }
}

impl<T> crate::subscription::subscribe::Fuse for Observable<T> {
    fn set_fused(&mut self, fused: bool, defused: bool) {
        self.fused = fused;
        self.defused = defused;
    }

    fn get_fused(&self) -> (bool, bool) {
        (self.fused, self.defused)
    }
}

impl<T: 'static> Subscribeable for Observable<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        let (fused, defused) = v.get_fused();

        if defused || (fused && !self.fused) {
            self.defused = v.defused;
            self.fused = v.fused;
        } else {
            v.set_fused(self.fused, self.defused);
        }
        (self.subscribe_fn)(v)
    }

    fn is_subject(&self) -> bool {
        false
    }
}

impl<O, T: 'static> ObservableExt<T> for O where O: Subscribeable<ObsType = T> {}

#[cfg(test)]
mod tests;
