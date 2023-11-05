use std::{
    any::Any,
    error::Error,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    thread::JoinHandle as ThreadJoinHandle,
};

use tokio::task::{self, JoinHandle};

use crate::observer::Observer;

/// A trait for types that can be subscribed to, allowing consumers to receive
/// values emitted by an observable stream.
pub trait Subscribeable {
    /// The type of items emitted by the observable stream.
    type ObsType;

    /// Subscribes to the observable stream and specifies how to handle emitted values.
    ///
    /// The `Subscriber` parameter defines the behavior for processing values emitted
    /// by the observable stream. The implementation of this method should establish
    /// the subscription and manage the delivery of values to the subscriber.
    ///
    /// The returned `Subscription` allows the subscriber to manage the subscription,
    /// such as unsubscribing or handling disposal.
    ///
    /// # Arguments
    ///
    /// - `s`: A `Subscriber` that handles emitted values and other events from
    ///        the observable stream.
    ///
    /// # Returns
    ///
    /// A `Subscription` that represents the subscription to the observable stream.
    fn subscribe(&mut self, s: Subscriber<Self::ObsType>) -> Subscription;

    /// Checks if the object implementing this trait is a variant of a `Subject`.
    ///
    /// Returns `true` if the object is a type of any `Subject` variant, indicating
    /// it can be treated as a subject. Returns `false` if the object is an `Observable`.
    fn is_subject(&self) -> bool {
        true
    }
}

/// A trait for types that can be unsubscribed, allowing the clean release of resources
/// associated with a subscription. This trait is typically used to signal the
/// `Observable` or `Subject` to stop emitting values.
pub trait Unsubscribeable {
    /// Unsubscribes from a subscription and releases associated resources.
    ///
    /// This method is called to gracefully terminate the subscription and release any
    /// resources held by it. Implementations should perform cleanup actions, such as
    /// closing connections, stopping timers, or deallocating memory.
    ///
    /// This method can also serve as a signal to notify the observable that it should
    /// stop emitting values. This is particularly relevant for asynchronous and/or
    /// multithreaded observables.
    ///
    /// The `Subscription` instance that this method is called on is consumed, making it
    /// unusable after the `unsubscribe` operation.
    fn unsubscribe(self);
}

/// A type that acts as an observer, allowing users to handle emitted values, errors,
/// and completion when subscribing to an `Observable` or `Subject`.
///
/// Users can create a `Subscriber` instance using the `new` method and provide
/// custom functions to handle the `next`, `error`, and `complete` events.
pub struct Subscriber<NextFnType> {
    next_fn: Box<dyn FnMut(NextFnType) + Send>,
    complete_fn: Option<Box<dyn FnMut() + Send + Sync>>,
    error_fn: Option<Box<dyn FnMut(Arc<dyn Error + Send + Sync>) + Send + Sync>>,
    completed: bool,
    fused: bool,
    pub(crate) defused: bool,
    errored: bool,
}

impl<NextFnType> Subscriber<NextFnType> {
    /// Creates a new `Subscriber` instance with custom handling functions for emitted
    /// values, errors, and completion.
    ///
    /// Error and completion handling functions are optional and can be specified based
    /// on requirements.
    pub fn new(
        next_fnc: impl FnMut(NextFnType) + 'static + Send,
        error_fnc: Option<impl FnMut(Arc<dyn Error + Send + Sync>) + 'static + Send + Sync>,
        complete_fnc: Option<impl FnMut() + 'static + Send + Sync>,
    ) -> Self {
        let mut s = Subscriber {
            next_fn: Box::new(next_fnc),
            complete_fn: None,
            error_fn: None,
            completed: false,
            fused: false,
            defused: false,
            errored: false,
        };

        if let Some(cfn) = complete_fnc {
            s.complete_fn = Some(Box::new(cfn));
        }
        if let Some(efn) = error_fnc {
            s.error_fn = Some(Box::new(efn));
        }
        s
    }

    // TODO: add is_fused() method.

    pub(crate) fn set_fused(&mut self, f: bool) {
        self.fused = f;
    }

    // pub(crate) fn set_defused(&mut self, d: bool) {
    //     self.defused = d;
    // }
}

impl<N> Observer for Subscriber<N> {
    type NextFnType = N;
    fn next(&mut self, v: Self::NextFnType) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        (self.next_fn)(v);
    }

    fn complete(&mut self) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        if let Some(cfn) = &mut self.complete_fn {
            (cfn)();
            self.completed = true;
        }
    }

    fn error(&mut self, observable_error: Arc<dyn Error + Send + Sync>) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        if let Some(efn) = &mut self.error_fn {
            (efn)(observable_error);
            self.errored = true;
        }
    }
}

type AwaitResult<T> = Result<T, Box<dyn Any + Send>>;

pub struct SubscriptionCollection {
    subscriptions: Arc<Mutex<Option<Vec<Subscription>>>>,
    signal_sent: Arc<RwLock<bool>>,
    use_task: bool,
}

impl SubscriptionCollection {
    pub(crate) fn new(s: Arc<Mutex<Option<Vec<Subscription>>>>, use_task: bool) -> Self {
        SubscriptionCollection {
            subscriptions: s,
            signal_sent: Arc::new(RwLock::new(false)),
            use_task,
        }
    }

    pub(crate) fn join_all(self) -> AwaitResult<()> {
        let mut subscriptionsh = self.subscriptions.lock().unwrap();

        let subscriptions = subscriptionsh.take();

        // Prepare channel for listening unsubscribing signal from `take()` operatior.
        let (tx, rx) = std::sync::mpsc::channel();

        *subscriptionsh = Some(vec![Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                // Send signal for unsubscribing.
                let _ = tx.send(true);
            })),
            SubscriptionHandle::Nil,
        )]);

        // Wait for unsubscribing signal.
        let cl = Arc::clone(&self.signal_sent);
        std::thread::spawn(move || {
            if let Ok(true) = rx.recv() {
                *cl.write().unwrap() = true;
            }
        });

        drop(subscriptionsh);

        if subscriptions.is_none() {
            return Ok(());
        }

        let mut stored = Vec::with_capacity(subscriptions.as_ref().unwrap().len());
        let mut stored_tasks = Vec::with_capacity(subscriptions.as_ref().unwrap().len());

        use std::thread::{sleep, spawn};

        for mut s in subscriptions.unwrap() {
            if *self.signal_sent.read().unwrap() {
                s.unsubscribe();
                continue;
            }

            match s.subscription_future {
                SubscriptionHandle::Nil => (),
                SubscriptionHandle::JoinThread(thread_handle) => {
                    s.subscription_future = SubscriptionHandle::Nil;
                    stored.push(s);

                    let h = spawn(|| thread_handle.join());
                    stored_tasks.push(h);
                }
                SubscriptionHandle::JoinSubscriptions(collection) => {
                    s.subscription_future = SubscriptionHandle::Nil;
                    stored.push(s);

                    let h = spawn(|| collection.join_all());
                    stored_tasks.push(h);
                }
                SubscriptionHandle::JoinTask(..) => {
                    panic!("Handle should be OS thread handle but it is Tokio task handle instead. When working with Tokio, use `join_thread_or_task().await` to await the completion of observables.");
                }
            }
        }

        let local_signal = Arc::new(RwLock::new(false));
        let local_signal_cloned = Arc::clone(&local_signal);
        spawn(move || loop {
            if *self.signal_sent.read().unwrap() {
                let r = stored.pop();
                if let Some(s) = r {
                    s.unsubscribe();
                }
            }

            if stored.is_empty() || *local_signal_cloned.read().unwrap() {
                break;
            }
            sleep(std::time::Duration::from_millis(5));
        });

        for s in stored_tasks {
            s.join()
                .unwrap_or(Err(Box::new("failed to await merged observable")))?;
        }
        *local_signal.write().unwrap() = true;
        Ok(())
    }

    pub(crate) fn join_all_async(self) -> Pin<Box<dyn Future<Output = AwaitResult<()>> + 'static>> {
        Box::pin(async move {
            let mut subscriptionsh = self.subscriptions.lock().unwrap();

            let subscriptions = subscriptionsh.take();

            if self.use_task {
                // Prepare channel for listening unsubscribing signal from `take()` operatior.
                let (tx, mut rx) = tokio::sync::mpsc::channel(5);

                *subscriptionsh = Some(vec![Subscription::new(
                    UnsubscribeLogic::Future(Box::pin(async move {
                        // Send signal for unsubscribing.
                        let _ = tx.send(true).await;
                    })),
                    SubscriptionHandle::Nil,
                )]);

                // Wait for unsubscribing signal.
                let cl = Arc::clone(&self.signal_sent);
                task::spawn(async move {
                    if let Some(true) = rx.recv().await {
                        *cl.write().unwrap() = true;
                    }
                });
            } else {
                // Prepare channel for listening unsubscribing signal from `take()` operatior.
                let (tx, rx) = std::sync::mpsc::channel();

                *subscriptionsh = Some(vec![Subscription::new(
                    UnsubscribeLogic::Logic(Box::new(move || {
                        // Send signal for unsubscribing.
                        let _ = tx.send(true);
                    })),
                    SubscriptionHandle::Nil,
                )]);

                // Wait for unsubscribing signal.
                let cl = Arc::clone(&self.signal_sent);
                std::thread::spawn(move || {
                    if let Ok(true) = rx.recv() {
                        *cl.write().unwrap() = true;
                    }
                });
            }

            drop(subscriptionsh);

            if subscriptions.is_none() {
                return Ok(());
            }

            let mut stored = Vec::with_capacity(subscriptions.as_ref().unwrap().len());
            let mut stored_tasks = Vec::with_capacity(subscriptions.as_ref().unwrap().len());

            for mut s in subscriptions.unwrap() {
                if *self.signal_sent.read().unwrap() {
                    s.unsubscribe();
                    continue;
                }

                match s.subscription_future {
                    SubscriptionHandle::Nil => (),
                    SubscriptionHandle::JoinTask(task_handle) => {
                        s.subscription_future = SubscriptionHandle::Nil;
                        stored.push(s);

                        let h = task::spawn(async {
                            let r = task_handle.await;
                            if r.is_err() {
                                return r.map_err(|e| Box::new(e) as Box<dyn Any + Send>);
                            }
                            Ok(())
                        });
                        stored_tasks.push(h);
                    }
                    SubscriptionHandle::JoinThread(thread_handle) => {
                        s.subscription_future = SubscriptionHandle::Nil;
                        stored.push(s);

                        let h = task::spawn(async { thread_handle.join() });
                        stored_tasks.push(h);
                    }
                    SubscriptionHandle::JoinSubscriptions(collection) => {
                        s.subscription_future = SubscriptionHandle::Nil;
                        stored.push(s);

                        let h = task::spawn_local(async { collection.join_all_async().await });
                        stored_tasks.push(h);
                    }
                }
            }

            let h = task::spawn(async move {
                loop {
                    if *self.signal_sent.read().unwrap() {
                        let r = stored.pop();
                        if let Some(s) = r {
                            s.unsubscribe();
                        }
                    }

                    if stored.is_empty() {
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                }
            });

            for s in stored_tasks {
                s.await
                    .unwrap_or(Err(Box::new("failed to await merged observable")))?;
            }
            h.abort();
            Ok(())
        })
    }
}

/// Enumeration representing different types of handles used by `Subscriber` to await
/// asynchronous tasks or threads.
pub enum SubscriptionHandle {
    /// No specific handle for task or thread awaiting.
    Nil,

    /// Holds a join handle for awaiting an asynchronous task.
    JoinTask(JoinHandle<()>),

    /// Holds a join handle for awaiting a thread.
    JoinThread(ThreadJoinHandle<()>),

    JoinSubscriptions(SubscriptionCollection),
}

// enum SubscriptionError {
//     JoinTaskError(JoinError),
//     JoinThreadError(Box<dyn Any + Send>),
// }

/// Represents a subscription to an observable or a subject, allowing control over
/// the subscription.
///
/// When an observable or subject is subscribed to, it typically returns a
/// `Subscription` instance. This subscription can be used to manage the subscription,
/// allowing for unsubscription or resource cleanup, and can also be used to await
/// asynchronous observables that use `Tokio` tasks or OS threads.
pub struct Subscription {
    pub(crate) unsubscribe_logic: UnsubscribeLogic,
    pub(crate) subscription_future: SubscriptionHandle,
}

impl Subscription {
    /// Creates a new Subscription instance with the specified unsubscribe logic and
    /// subscription handle.
    ///
    /// The `unsubscribe_logic` parameter defines the logic to execute upon
    /// unsubscribing from the observable. See [`UnsubscribeLogic`] for more details
    /// on available unsubscribe strategies.
    ///
    /// The `subscription_future` parameter holds a handle for awaiting asynchronous
    /// tasks or threads associated with the subscription. See [`SubscriptionHandle`]
    /// for details on the types of handles.
    ///
    /// [`UnsubscribeLogic`]: enum.UnsubscribeLogic.html
    /// [`SubscriptionHandle`]: enum.SubscriptionHandle.html
    pub fn new(
        unsubscribe_logic: UnsubscribeLogic,
        subscription_future: SubscriptionHandle,
    ) -> Self {
        Subscription {
            unsubscribe_logic,
            subscription_future,
        }
    }

    /// Awaits the completion of the asynchronous task or thread associated with
    /// this subscription.
    ///
    /// If the observable uses asynchronous `Tokio` tasks, this method will await the
    /// completion of the task. If the observable uses OS threads, it will await the
    /// completion of the thread.
    pub async fn join_thread_or_task(self) -> Result<(), Box<dyn Any + Send>> {
        match self.subscription_future {
            SubscriptionHandle::JoinTask(task_handle) => {
                let r = task_handle.await;
                r.map_err(|e| Box::new(e) as Box<dyn Any + Send>)
            }
            SubscriptionHandle::JoinThread(thread_handle) => thread_handle.join(),
            SubscriptionHandle::JoinSubscriptions(s) => s.join_all_async().await,
            SubscriptionHandle::Nil => Ok(()),
        }
    }

    /// Awaits the completion of the asynchronous OS thread associated with this subscription.
    ///
    /// This method is used to await the completion of an asynchronous observable
    /// that uses an OS thread for its processing. It will block the current thread
    /// until the observable, using an OS thread, has completed its task.
    ///
    /// This method is useful when using `rxr` without `Tokio` in a project, as it
    /// allows for awaiting completion without relying on asynchronous constructs.
    ///
    /// # Panics
    ///
    /// If this method is used to await a `Tokio` task, it will panic.
    ///
    /// To await `Tokio` tasks without causing a panic, use the `join_thread_or_task`
    /// method instead.
    pub fn join_thread(self) -> Result<(), Box<dyn Any + Send>> {
        match self.subscription_future {
            SubscriptionHandle::JoinThread(thread_handle) => thread_handle.join(),
            SubscriptionHandle::Nil => Ok(()),
            SubscriptionHandle::JoinSubscriptions(s) => s.join_all(),
            SubscriptionHandle::JoinTask(_) => {
                panic!("Handle should be OS thread handle but it is Tokio task handle instead. When working with Tokio, use `join_thread_or_task().await` to await the completion of observables.")
            }
        }
    }
}

impl Unsubscribeable for Subscription {
    fn unsubscribe(self) {
        // match self.unsubscribe_logic {
        //     UnsubscribeLogic::Nil => (),
        //     UnsubscribeLogic::Logic(fnc) => fnc(),
        //     UnsubscribeLogic::Wrapped(subscription) => subscription.unsubscribe(),
        //     UnsubscribeLogic::Future(future) => {
        //         tokio::task::spawn(async {
        //             future.await;
        //         });
        //     },
        // }
        self.unsubscribe_logic.unsubscribe();
    }
}

/// Enumerates various unsubscribe logic options for a subscription.
pub enum UnsubscribeLogic {
    /// No specific unsubscribe logic.
    Nil,

    /// If one subscription depends on another. Wrapped subscription's unsubscribe
    /// will be called upon unsubscribing.
    Wrapped(Box<Subscription>),

    /// Unsubscribe logic defined by a function.
    Logic(Box<dyn FnOnce() + Send>),

    /// Asynchronous unsubscribe logic represented by a future. Use if you need to
    /// spawn `Tokio` tasks or `.await` as a part of the unsubscribe logic.
    Future(Pin<Box<dyn Future<Output = ()> + Send>>),
}

impl UnsubscribeLogic {
    fn unsubscribe(mut self) -> Self {
        match self {
            UnsubscribeLogic::Nil => (),
            UnsubscribeLogic::Logic(fnc) => {
                fnc();
                self = Self::Nil;
            }
            UnsubscribeLogic::Wrapped(subscription) => {
                subscription.unsubscribe();
                self = Self::Nil;
            }
            UnsubscribeLogic::Future(future) => {
                tokio::task::spawn(async {
                    future.await;
                });
                self = Self::Nil;
            }
        }
        self
    }
}
