use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use crate::{
    observer::Observer,
    subscribe::Unsubscribeable,
    subscription::subscribe::{
        Subscribeable, Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic,
    },
    Observable,
};

/// A specialized `Subject` variant emits its latest value to observers upon completion.
///
/// `AsyncSubject` captures and broadcasts the last emitted value from a source
/// observable, but this broadcasting occurs only after the source observable
/// completes. It sends this value to all new subscriptions.
///
/// If an error is invoked in the source observable, the `AsyncSubject` will not emit
/// the latest value to subscriptions. Instead, it propagates the error notification
/// from the source `Observable` to all subscriptions. This ensures that existing and
/// new subscriptions are properly informed about the error, maintaining consistent
/// error handling across observers.
///
/// In `rxr`, this type is primarily used for calling its `emitter_receiver` function,
/// and then you use the returned [`AsyncSubjectEmitter`] to emit values, while
/// using [`AsyncSubjectReceiver`] to subscribe to those values.
///
/// [`AsyncSubjectEmitter`]: struct.AsyncSubjectEmitter.html
/// [`AsyncSubjectReceiver`]: struct.AsyncSubjectReceiver.html
///
/// # Examples
///
/// AsyncSubject completion
///
///```no_run
/// use rxr::{subjects::AsyncSubject, subscribe::Subscriber};
/// use rxr::{ObservableExt, Observer, Subscribeable};
///
/// pub fn create_subscriber(subscriber_id: i32) -> Subscriber<i32> {
///     Subscriber::new(
///         move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
///         |_| eprintln!("Error"),
///         move || println!("Completed {}", subscriber_id),
///     )
/// }
///
/// // Initialize a `AsyncSubject` and obtain its emitter and receiver.
/// let (mut emitter, mut receiver) = AsyncSubject::emitter_receiver();
///
/// // Registers `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Stores 101 ast the latest value.
/// emitter.next(102); // Latest value is now 102.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `AsyncSubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |_| eprintln!("Error"),
///         || println!("Completed 2"),
///     ));
///
/// // Registers `Subscriber` 3.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Latest value is now 103.
///
/// // Emits latest value (103) to registered `Subscriber`'s 1, 2 and 3 and calls
/// // `complete` on each of them.
/// emitter.complete();
///
/// // Subscriber 4: post-completion subscribe, emits latest value (103) and completes.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called post-completion, does not emit.
///```
///
/// AsyncSubject error
///
///```no_run
/// use std::error::Error;
/// use std::fmt::Display;
/// use std::sync::Arc;
///
/// use rxr::{subjects::AsyncSubject, subscribe::Subscriber};
/// use rxr::{ObservableExt, Observer, Subscribeable};
///
/// pub fn create_subscriber(subscriber_id: i32) -> Subscriber<i32> {
///     Subscriber::new(
///         move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
///         move |e| eprintln!("Error: {} {}", e, subscriber_id),
///         || println!("Completed"),
///     )
/// }
///
/// #[derive(Debug)]
/// struct AsyncSubjectError(String);
///
/// impl Display for AsyncSubjectError {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.0)
///     }
/// }
///
/// impl Error for AsyncSubjectError {}
///
/// // Initialize a `AsyncSubject` and obtain its emitter and receiver.
/// let (mut emitter, mut receiver) = AsyncSubject::emitter_receiver();
///
/// // Registers `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Stores 101 ast the latest value.
/// emitter.next(102); // Latest value is now 102.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `AsyncSubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |e| eprintln!("Error: {} 2", e),
///         || println!("Completed"),
///     ));
///
/// // Registers `Subscriber` 3.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Latest value is now 103.
///
/// // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
/// emitter.error(Arc::new(AsyncSubjectError(
///     "AsyncSubject error".to_string(),
/// )));
///
/// // Subscriber 4: subscribed after subject's error call; emits error and
/// // does not emit further.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called post-completion, does not emit.
///```
pub struct AsyncSubject<T> {
    value: Option<T>,
    observers: Vec<(u64, Subscriber<T>)>,
    // fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: Send + Sync + 'static> AsyncSubject<T> {
    /// Initializes an `AsyncSubject` and returns a tuple containing an
    /// `AsyncSubjectEmitter` for emitting values and an `AsyncSubjectReceiver`
    /// for subscribing to emitted values.
    pub fn emitter_receiver() -> (AsyncSubjectEmitter<T>, AsyncSubjectReceiver<T>) {
        let s = Arc::new(Mutex::new(AsyncSubject {
            value: None,
            observers: Vec::with_capacity(16),
            // fused: false,
            completed: false,
            closed: false,
            error: None,
        }));

        (
            AsyncSubjectEmitter(Arc::clone(&s)),
            AsyncSubjectReceiver(Arc::clone(&s)),
        )
    }
}

/// Subscription handler for `AsyncSubject`.
///
/// `AsyncSubjectReceiver` acts as an `Observable`, allowing you to utilize its
/// `subscribe` method for receiving emissions from the `AsyncSubject`'s multicasting.
/// You can also employ its `unsubscribe` method to close the `AsyncSubject` and
/// remove registered observers.
#[derive(Clone)]
pub struct AsyncSubjectReceiver<T>(Arc<Mutex<AsyncSubject<T>>>);

// Multicasting emitter for `AsyncSubject`.
///
/// `AsyncSubjectEmitter` acts as an `Observer`, allowing you to utilize its `next`,
/// `error`, and `complete` methods for multicasting emissions to all registered
/// observers within the `AsyncSubject`.
#[derive(Clone)]
pub struct AsyncSubjectEmitter<T>(Arc<Mutex<AsyncSubject<T>>>);

impl<T> AsyncSubjectReceiver<T> {
    /// Returns the number of registered observers.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().observers.len()
    }

    /// Returns `true` if no observers are registered, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // pub(crate) fn fuse(self) -> Self {
    //     for (_, o) in &mut self.0.lock().unwrap().observers {
    //         o.set_fused(true);
    //     }
    //     self
    // }

    // pub(crate) fn defuse(self) -> Self {
    //     for (_, o) in &mut self.0.lock().unwrap().observers {
    //         o.set_fused(false);
    //     }
    //     self
    // }
}

impl<T> crate::subscription::subscribe::Fuse for AsyncSubjectReceiver<T> {}

impl<T: Clone + Send + Sync + 'static> Subscribeable for AsyncSubjectReceiver<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        let key: u64 = super::gen_key().next().unwrap_or(super::random_seed());

        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // if src.fused {
            //     v.set_fused(true);
            // }
            // If AsyncSubject is completed do not register new Subscriber.
            if src.completed {
                if let Some(err) = &src.error {
                    // AsyncSubject completed with error. Call error() on
                    // every subsequent Subscriber.
                    v.error(Arc::clone(err));
                } else {
                    // AsyncSubject completed. Emit stored value if there is one and
                    // call complete() on every subsequent Subscriber.
                    if let Some(value) = &src.value {
                        v.next(value.clone());
                    }
                    v.complete();
                }
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // Register Subscriber.
            src.observers.push((key, v));
        } else {
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned
                    .lock()
                    .unwrap()
                    .observers
                    .retain(move |v| v.0 != key);
            })),
            SubscriptionHandle::Nil,
        )
    }
}

impl<T> Unsubscribeable for AsyncSubjectReceiver<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for AsyncSubjectEmitter<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            // Store new value in AsyncSubject.
            src.value = Some(v);
        }
    }

    fn error(&mut self, e: Arc<dyn Error + Send + Sync>) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            for (_, o) in &mut src.observers {
                o.error(e.clone());
            }
            src.completed = true;
            src.error = Some(e);
            src.observers.clear();
        }
    }

    fn complete(&mut self) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            src.completed = true;
            if let Some(value) = &src.value {
                let v = value.clone();
                for (_, o) in &mut src.observers {
                    o.next(v.clone());
                }
            }
            for (_, o) in &mut src.observers {
                o.complete();
            }
            src.observers.clear();
        }
    }
}

impl<T: Clone + Send + 'static> From<AsyncSubjectEmitter<T>> for Subscriber<T> {
    fn from(mut value: AsyncSubjectEmitter<T>) -> Self {
        let mut vn = value.clone();
        let mut ve = value.clone();
        Subscriber::new(
            move |v| {
                vn.next(v);
            },
            move |e| ve.error(e),
            move || value.complete(),
        )
    }
}

impl<T: Clone + Send + Sync + 'static> From<AsyncSubjectReceiver<T>> for Observable<T> {
    fn from(mut value: AsyncSubjectReceiver<T>) -> Self {
        Observable::new(move |subscriber| value.subscribe(subscriber))
    }
}
