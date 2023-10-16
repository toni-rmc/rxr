#![allow(clippy::needless_doctest_main)]
//! The `observable` module provides the building blocks for creating and manipulating
//! observables, allowing for reactive programming in Rust.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::subscription::subscribe::{
    Subscribeable, Subscriber, Subscription, SubscriptionHandle, Unsubscribeable,
};
use crate::{observer::Observer, subscription::subscribe::UnsubscribeLogic};

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
/// // Create the Subscriber with a mandatory `next` function, and optional
/// // `error` and `complete` functions.
/// let observer = Subscriber::new(
///     |v| println!("Emitted {}", v),
///     // No need for the `error` function in this simple example, but we
///     // have to type annotate `None`.
///     None::<fn(_)>,
///     // The `complete` function is optional, so we wrap it in `Some()`.
///     // Alternatively, we can skip the `complete` function entirely by
///     // passing `None::<fn()>`.
///     Some(|| println!("Completed")),
/// );
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
/// // `error` and `complete` functions.
/// let observer = Subscriber::new(
///     |v| println!("Emitted {}", v),
///     // No need for error function in this simple example, but we
///     // have to type annotate `None`.
///     None::<fn(_)>,
///     // The `complete` function is optional, so we wrap it in `Some()`.
///     // Alternatively, we can skip the `complete` function entirely by
///     // passing `None::<fn()>`.
///     Some(|| println!("Completed")),
/// );
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
/// if subscription.join_thread().is_err() {
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
/// // `error` and `complete` functions.
/// let observer = Subscriber::new(
///     |v| println!("Emitted {}", v),
///     // No need for error function in this simple example, but we
///     // have to type annotate `None`.
///     None::<fn(_)>,
///     // The `complete` function is optional, so we wrap it in `Some()`.
///     // Alternatively, we can skip the `complete` function entirely by
///     // passing `None::<fn()>`.
///     Some(|| println!("Completed")),
/// );
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
///     // `error` and `complete` functions.
///     let observer = Subscriber::new(
///         |v| println!("Emitted {}", v),
///         // No need for error function in this simple example, but we
///         // have to type annotate `None`.
///         None::<fn(_)>,
///         // The `complete` function is optional, so we wrap it in `Some()`.
///         // Alternatively, we can skip the `complete` function entirely by
///         // passing `None::<fn()>`.
///         Some(|| println!("Completed")),
///     );
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
///     if subscription.join_thread_or_task().await.is_err() {
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
///     Some(|e| eprintln!("{}", e)),
///     Some(|| println!("User input handled")),
/// );
///
/// let mut observable = get_less_than_100();
///
/// observable.subscribe(observer);
///```
pub struct Observable<T> {
    subscribe_fn: Box<dyn FnMut(Subscriber<T>) -> Subscription + Send + Sync>,
    fused: bool,
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
        }
    }

    pub fn fuse(mut self) -> Self {
        self.fused = true;
        self
    }

    pub fn defuse(mut self) -> Self {
        self.fused = false;
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    let t = f(v);
                    o_shared.lock().unwrap().next(t);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    if predicate(&v) {
                        o_shared.lock().unwrap().next(v);
                    }
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    /// Skips the first `n` items emitted by the observable and then emits the rest.
    ///
    /// If `n` is greater than or equal to the total number of items, it behaves as
    /// if the observable is complete and emits no items.
    fn skip(mut self, n: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut n = n;
            let u = Subscriber::new(
                move |v| {
                    if n > 0 {
                        n -= 1;
                        return;
                    }
                    o_shared.lock().unwrap().next(v);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    std::thread::sleep(Duration::from_millis(num_of_ms));
                    o_shared.lock().unwrap().next(v);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        let mut i = 0;

        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let (tx, rx) = std::sync::mpsc::channel();
            let mut signal_sent = false;

            // Alternative implementation for Subject's if desired behavior is to skip
            // call to unsubscribe() when take() operator is used. This might be used
            // for performance reasons because opening a channel and spawning a new thread
            // can be skipped when this operator is used on Subject's.
            if self.is_subject() {
                 signal_sent = true;
            }

            let u = Subscriber::new(
                move |v| {
                    if i < n {
                        i += 1;
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        // let tx = tx.clone();
                        signal_sent = true;
                        // std::thread::spawn(move || {
                        // println!("Send >>>>>>>>>>>>>>>");
                        let _ = tx.send(true);
                        // });
                    }
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );

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

            // Check if unsubscribe logic is Future so we can use Tokio tasks.
            if let UnsubscribeLogic::Future(_) = &unsubscriber.unsubscribe_logic {
                let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
                let u_cloned = Arc::clone(&unsubscriber);

                tokio::task::spawn(async move {
                    if rx.recv().is_ok() {
                        // println!("SIGNAL received");
                        if let Some(s) = unsubscriber.lock().unwrap().take() {
                            // println!("UNSUBSCRIBE called");
                            s.unsubscribe();
                        }
                    }
                });

                return Subscription::new(
                    UnsubscribeLogic::Logic(Box::new(move || {
                        if let Some(s) = u_cloned.lock().unwrap().take() {
                            s.unsubscribe();
                        }
                    })),
                    ijh,
                );
            }

            // If unsubscribe logic is not Future we use OS threads instead.
            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            let _ = std::thread::spawn(move || {
                if rx.recv().is_ok() {
                    // println!("SIGNAL received");
                    if let Some(s) = unsubscriber.lock().unwrap().take() {
                        // println!("UNSUBSCRIBE called");
                        s.unsubscribe();
                    }
                }
                // println!("RECEIVER dropped in take()");
            });

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                ijh,
            )
        })
    }

    /// Merges the current observable with a vector of observables, emitting items
    /// from all of them concurrently.
    fn merge(mut self, mut sources: Vec<Observable<T>>) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static
    {
        fn wrap_subscriber<S: 'static>(s: Arc<Mutex<Subscriber<S>>>) -> Subscriber<S>{
            let s_complete = s.clone();
            let s_error = s.clone();

            Subscriber::new(move |v| {
                s.lock().unwrap().next(v);
            }, Some(move |e| {
                s_error.lock().unwrap().error(e);
            }), Some(move || {
                s_complete.lock().unwrap().complete();
            }))
        }

        Observable::new(move |o| {
            let o = Arc::new(Mutex::new(o));
            let mut subscriptions = Vec::with_capacity(sources.len());

            self.subscribe(wrap_subscriber(o.clone()));

            for source in &mut sources {
                let wrapped = wrap_subscriber(o.clone());
                let subscription = source.subscribe(wrapped);
                subscriptions.push(subscription);
            }

            // TODO: add awaiting to merged Subscriber's, maybe by using special
            // object wich has `Option<Vec<Subscription>>` with methods for waiting
            // on all observables to complete.
            Subscription::new(UnsubscribeLogic::Logic(Box::new(|| {
                for subscription in subscriptions {
                    subscription.unsubscribe();
                }
            })), SubscriptionHandle::Nil)
        })
    }

    /// Merges the current observable with another observable, emitting items from
    /// both concurrently.
    fn merge_one(mut self, mut source: Observable<T>) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static
    {
        fn wrap_subscriber<S: 'static>(s: Arc<Mutex<Subscriber<S>>>) -> Subscriber<S>{
            let s_complete = s.clone();
            let s_error = s.clone();

            Subscriber::new(move |v| {
                s.lock().unwrap().next(v);
            }, Some(move |e| {
                s_error.lock().unwrap().error(e);
            }), Some(move || {
                s_complete.lock().unwrap().complete();
            }))
        }

        Observable::new(move |o| {
            let o = Arc::new(Mutex::new(o));

            let wrapped = wrap_subscriber(o.clone());
            let wrapped2 = wrap_subscriber(o.clone());

            let s1 = self.subscribe(wrapped);
            let s2 = source.subscribe(wrapped2);

            Subscription::new(UnsubscribeLogic::Logic(Box::new(|| {
                s1.unsubscribe();
                s2.unsubscribe();
            })), SubscriptionHandle::Nil)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let mut current_subscription: Option<Subscription> = None;

            let u = Subscriber::new(
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
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                        }),
                    );

                    if let Some(subscription) = current_subscription.take() {
                        subscription.unsubscribe();
                    }

                    let s = inner_observable.subscribe(inner_subscriber);
                    current_subscription = Some(s);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let u = Subscriber::new(
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
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                        }),
                    );
                    inner_observable.subscribe(inner_subscriber);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let pending_observables: Arc<Mutex<VecDeque<(Observable<R>, Subscriber<R>)>>> =
                Arc::new(Mutex::new(VecDeque::new()));

            let mut first_pass = true;

            let u = Subscriber::new(
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
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                            if let Some((mut io, is)) = po_cloned.lock().unwrap().pop_front() {
                                io.subscribe(is);
                            }
                        }),
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
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
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
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let active_subscription = Arc::new(Mutex::new(false));
            let guard = Arc::new(Mutex::new(true));

            let u = Subscriber::new(
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
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();

                            // Mark this inner subscription as completed so that next
                            // one can be allowed to emit all of it's values.
                            *as_cloned2.lock().unwrap() = false;
                        }),
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
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }
}

impl<T: 'static> Subscribeable for Observable<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if self.fused {
            v.set_fused(true);
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
