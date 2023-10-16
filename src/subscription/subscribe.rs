use std::{
    any::Any, error::Error, future::Future, pin::Pin, sync::Arc,
    thread::JoinHandle as ThreadJoinHandle,
};

use tokio::task::JoinHandle;

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

    pub(crate) fn set_fused(&mut self, f: bool) {
        self.fused = f;
    }
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
/// Enumeration representing different types of handles used by `Subscriber` to await
/// asynchronous tasks or threads.
pub enum SubscriptionHandle {
    /// No specific handle for task or thread awaiting.
    Nil,

    /// Holds a join handle for awaiting an asynchronous task.
    JoinTask(JoinHandle<()>),

    /// Holds a join handle for awaiting a thread.
    JoinThread(ThreadJoinHandle<()>),
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
            SubscriptionHandle::JoinTask(_) => {
                panic!("handle should be OS thread handle but it is tokio task handle instead")
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
    Future(Pin<Box<dyn Future<Output = ()> + Send + Sync>>),
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
