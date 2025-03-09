use std::{
    any::Any,
    error::Error,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    thread::JoinHandle as ThreadJoinHandle,
};

use tokio::runtime;
use tokio::task::{self, JoinHandle};

use crate::observer::Observer;

#[doc(hidden)]
pub trait Fuse {
    fn set_fused(&mut self, _: bool, _: bool) {}

    fn get_fused(&self) -> (bool, bool) {
        (false, false)
    }
}

/// A trait for types that can be subscribed to, allowing consumers to receive
/// values emitted by an observable stream.
pub trait Subscribeable: Fuse {
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

    #[doc(hidden)]
    fn set_subject_indicator(&mut self, _: bool) {}
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

type NextFn<T> = Box<dyn FnMut(T) + Send>;
type CompleteFn = Box<dyn FnMut() + Send + Sync>;
type ErrorFn = Box<dyn FnMut(Arc<dyn Error + Send + Sync>) + Send + Sync>;

/// A type that acts as an observer, allowing users to handle emitted values, errors,
/// and completion when subscribing to an `Observable` or `Subject`.
///
/// Users can create a `Subscriber` instance using the `new` method and provide
/// custom functions to handle the `next`, `error`, and `complete` events.
#[allow(clippy::struct_excessive_bools)]
pub struct Subscriber<NextFnType> {
    next_fn: NextFn<NextFnType>,
    complete_fn: Option<CompleteFn>,
    error_fn: Option<ErrorFn>,
    completed: bool,
    pub(crate) fused: bool,
    pub(crate) defused: bool,
    pub(crate) take_wrapped: bool,
    errored: bool,
}

impl<NextFnType> Subscriber<NextFnType> {
    /// Creates a new `Subscriber` instance with custom handling functions for emitted
    /// values, errors, and completion.
    pub fn new(
        next_fn: impl FnMut(NextFnType) + 'static + Send,
        error_fn: impl FnMut(Arc<dyn Error + Send + Sync>) + 'static + Send + Sync,
        complete_fn: impl FnMut() + 'static + Send + Sync,
    ) -> Self {
        Subscriber {
            next_fn: Box::new(next_fn),
            complete_fn: Some(Box::new(complete_fn)),
            error_fn: Some(Box::new(error_fn)),
            completed: false,
            fused: false,
            defused: false,
            take_wrapped: false,
            errored: false,
        }
    }

    /// Create a new Subscriber with the provided `next` function.
    ///
    /// The `next` closure is called when the observable emits a new item. It takes
    /// a parameter of type `NextFnType`, which is an item emitted by the observable.
    pub fn on_next(next_fn: impl FnMut(NextFnType) + 'static + Send) -> Self {
        Subscriber {
            next_fn: Box::new(next_fn),
            complete_fn: None,
            error_fn: None,
            completed: false,
            fused: false,
            defused: false,
            take_wrapped: false,
            errored: false,
        }
    }

    /// Set the completion function for the Subscriber.
    ///
    /// The provided closure will be called when the observable completes its
    /// emission sequence.
    pub fn on_complete(&mut self, complete_fn: impl FnMut() + 'static + Send + Sync) {
        self.complete_fn = Some(Box::new(complete_fn));
    }

    /// Set the error-handling function for the Subscriber.
    ///
    /// The provided closure will be called when the observable encounters an error
    /// during its emission sequence. It takes an `Arc` wrapping a trait object that
    /// implements the `Error`, `Send`, and `Sync` traits as its parameter.
    pub fn on_error(
        &mut self,
        error_fn: impl FnMut(Arc<dyn Error + Send + Sync>) + 'static + Send + Sync,
    ) {
        self.error_fn = Some(Box::new(error_fn));
    }

    /// If you fuse an observable this method will return true. When making your
    /// observable, you can use this method to check if observable is fused or not.
    ///
    /// ```text
    /// Observable::new(|subscriber| {
    ///     // ...
    ///     if subscriber.is_fused() { ... };
    ///     // ...
    /// });
    /// ```
    #[must_use]
    pub fn is_fused(&self) -> bool {
        self.fused && !self.defused
    }

    // pub(crate) fn set_fused(&mut self, f: bool) {
    //     self.fused = f;
    // }

    // pub(crate) fn set_defused(&mut self, d: bool) {
    //     self.defused = d;
    // }
}

impl<T> Fuse for Subscriber<T> {
    fn set_fused(&mut self, fused: bool, defused: bool) {
        // if fused {
        //     self.fused = true;
        //     self.defused = false;
        // } else {
        //     self.fused = false;
        //     self.defused = true;
        // }

        // let mut wrapped = &self.wrapped;

        // while let x@Some(s) = wrapped {
        //     s.lock().unwrap().set_fused_inner(fused);
        //     wrapped = x;
        // }
        self.fused = fused;
        self.defused = defused;
    }

    fn get_fused(&self) -> (bool, bool) {
        // self.fused && !self.defused
        (self.fused, self.defused)
    }
}

impl<T> Observer for Subscriber<T> {
    type NextFnType = T;
    fn next(&mut self, v: Self::NextFnType) {
        if (!self.take_wrapped && self.errored)
            || (!self.take_wrapped && self.fused && self.completed)
        {
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
    signal_sent: Arc<Mutex<bool>>,
    pub(crate) use_task: bool,
}

impl SubscriptionCollection {
    pub(crate) fn new(s: Arc<Mutex<Option<Vec<Subscription>>>>, use_task: bool) -> Self {
        SubscriptionCollection {
            subscriptions: s,
            signal_sent: Arc::new(Mutex::new(false)),
            use_task,
        }
    }

    pub(crate) fn join_all(self) -> AwaitResult<()> {
        use std::thread::{sleep, spawn};

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
                *cl.lock().unwrap() = true;
            }
        });

        drop(subscriptionsh);

        if subscriptions.is_none() {
            return Ok(());
        }

        let mut stored = Vec::with_capacity(subscriptions.as_ref().unwrap().len());
        let mut stored_tasks = Vec::with_capacity(subscriptions.as_ref().unwrap().len());

        for mut s in subscriptions.unwrap() {
            if *self.signal_sent.lock().unwrap() {
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
                    panic!("Handle should be OS thread handle but it is Tokio task handle instead. When working with Tokio, use `join_concurrent().await` to await the completion of observables.");
                }
            }
        }

        let local_signal = Arc::new(RwLock::new(false));
        let local_signal_cloned = Arc::clone(&local_signal);
        spawn(move || loop {
            if *self.signal_sent.lock().unwrap() {
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

    #[allow(clippy::await_holding_lock)]
    #[allow(clippy::too_many_lines)]
    pub(crate) fn join_all_async(self) -> Pin<Box<dyn Future<Output = AwaitResult<()>> + 'static>> {
        Box::pin(async move {
            let mut subscriptionsh = self.subscriptions.lock().unwrap();

            let subscriptions = subscriptionsh.take();
            let signal_sent = Arc::clone(&self.signal_sent);
            let signal_sent_cl = Arc::clone(&signal_sent);
            let signal_sent_cl2 = Arc::clone(&signal_sent);

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
                task::spawn(async move {
                    if let Some(true) = rx.recv().await {
                        *signal_sent.lock().unwrap() = true;
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
                std::thread::spawn(move || {
                    if let Ok(true) = rx.recv() {
                        *signal_sent.lock().unwrap() = true;
                    }
                });
            }

            drop(subscriptionsh);

            if subscriptions.is_none() {
                return Ok(());
            }

            let mut stored = Vec::with_capacity(subscriptions.as_ref().unwrap().len());
            let mut stored_tasks = Vec::with_capacity(subscriptions.as_ref().unwrap().len());
            let mut stored_threads = Vec::with_capacity(subscriptions.as_ref().unwrap().len());

            for mut s in subscriptions.unwrap() {
                if *self.signal_sent.lock().unwrap() {
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
                        stored_threads.push(thread_handle);
                    }
                    SubscriptionHandle::JoinSubscriptions(collection) => {
                        s.subscription_future = SubscriptionHandle::Nil;
                        stored.push(s);

                        let h = task::spawn_local(async { collection.join_all_async().await });
                        stored_tasks.push(h);
                    }
                }
            }

            let mut tokio_current_thread = false;
            if let runtime::RuntimeFlavor::CurrentThread =
                runtime::Handle::current().runtime_flavor()
            {
                tokio_current_thread = true;
            }

            let stop_thread_event_loop = Arc::new(RwLock::new(false));
            let stop_thread_event_loop_cl = Arc::clone(&stop_thread_event_loop);

            if tokio_current_thread {
                // If flavor is `current_thread` move Subscribers from observables
                // that use OS threads into their own `Vec`.
                let mut stored_threads_subscriptions = Vec::with_capacity(8);
                let mut i = 0;
                while i < stored.len() {
                    if let UnsubscribeLogic::Logic(_) = &mut stored[i].unsubscribe_logic {
                        stored_threads_subscriptions.push(stored.remove(i));
                    } else {
                        i += 1;
                    }
                }
                // Start event loop in OS thread to accept unsubscribe signal from
                // async observables that use OS threads.
                std::thread::spawn(move || loop {
                    if *signal_sent_cl2.lock().unwrap() {
                        let r = stored_threads_subscriptions.pop();
                        if let Some(s) = r {
                            s.unsubscribe();
                        }
                    }

                    if stored_threads_subscriptions.is_empty()
                        || *stop_thread_event_loop_cl.read().unwrap()
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                });
            }

            // Start Tokio task event loop to await for unsubscribe signal from all
            // async observables or only from async observables that use Tokio tasks
            // if runtime flavor is `current_thread`.
            let h = task::spawn(async move {
                loop {
                    if *signal_sent_cl.lock().unwrap() {
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
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

            for s in stored_threads {
                s.join()?;
            }
            // Stop event loops.
            *stop_thread_event_loop.write().unwrap() = true;
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

    /// Holds a join handle for awaiting an asynchronous observable using Tokio task.
    JoinTask(JoinHandle<()>),

    /// Holds a join handle for awaiting an asynchronous observable using OS thread.
    JoinThread(ThreadJoinHandle<()>),

    /// Holds a subscription collection used for awaiting all merged observables.
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
#[allow(clippy::used_underscore_binding)]
pub struct Subscription {
    pub(crate) unsubscribe_logic: UnsubscribeLogic,
    pub(crate) subscription_future: SubscriptionHandle,
    pub(crate) runtime_handle: Result<runtime::Handle, runtime::TryCurrentError>,
    _is_subject: bool,
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
    #[must_use]
    pub fn new(
        unsubscribe_logic: UnsubscribeLogic,
        subscription_future: SubscriptionHandle,
    ) -> Self {
        let runtime_handle = tokio::runtime::Handle::try_current();
        Subscription {
            unsubscribe_logic,
            subscription_future,
            runtime_handle,
            _is_subject: false,
        }
    }

    pub(crate) fn subject_subscription(
        unsubscribe_logic: UnsubscribeLogic,
        subscription_future: SubscriptionHandle,
    ) -> Self {
        let runtime_handle = tokio::runtime::Handle::try_current();
        Subscription {
            unsubscribe_logic,
            subscription_future,
            runtime_handle,
            _is_subject: true,
        }
    }

    pub(crate) fn _is_subject(&self) -> bool {
        self._is_subject
    }

    /// Awaits the completion of the asynchronous task or thread associated with
    /// this subscription.
    ///
    /// If the observable uses asynchronous `Tokio` tasks, this method will await the
    /// completion of the task. If the observable uses OS threads, it will await the
    /// completion of the thread.
    ///
    /// # Errors
    ///
    /// Returns an error if joining a thread or awaiting a task used by the
    /// observable fails.
    pub async fn join_concurrent(self) -> Result<(), Box<dyn Any + Send>> {
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
    /// # Errors
    ///
    /// Returns an error if joining a thread used by the observable fails.
    ///
    /// # Panics
    ///
    /// If this method is used to await a `Tokio` task, it will panic.
    ///
    /// To await `Tokio` tasks without causing a panic, use the `join_concurrent().await`
    /// method instead.
    pub fn join(self) -> Result<(), Box<dyn Any + Send>> {
        match self.subscription_future {
            SubscriptionHandle::JoinThread(thread_handle) => thread_handle.join(),
            SubscriptionHandle::Nil => Ok(()),
            SubscriptionHandle::JoinSubscriptions(s) => s.join_all(),
            SubscriptionHandle::JoinTask(_) => {
                panic!("Handle should be OS thread handle but it is Tokio task handle instead. When working with Tokio, use `join_concurrent().await` to await the completion of observables.")
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
        self.unsubscribe_logic.unsubscribe(self.runtime_handle);
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
    fn unsubscribe(
        mut self,
        runtime_handle: Result<runtime::Handle, runtime::TryCurrentError>,
    ) -> Self {
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
                match runtime_handle {
                    Ok(handle) => {
                        handle.spawn(async {
                            future.await;
                        });
                    }
                    e @ Err(_) => {
                        e.expect(
                            "Observable that uses Tokio tasks is called outside of Tokio runtime",
                        );
                    }
                }
                self = Self::Nil;
            }
        }
        self
    }
}
