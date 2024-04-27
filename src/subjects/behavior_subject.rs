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

/// A variation of `Subject` that expects an initial value and emits its current value
/// whenever it is subscribed to.
///
/// The current value is either the provided default value in the case of no emits or
/// the most recently emitted value. When initializing a `BehaviorSubject`, you must
/// provide an initial value. In addition to emitting the most recent value to new
/// subscribers, `BehaviorSubject` broadcasts this value to all subscribers upon subscription
///
/// If an error is encountered in the source observable, `BehaviorSubject` will not
/// emit any items to future subscriptions. Instead, it will just pass along the
/// error notification from the source observable to these new subscriptions.
///
/// In `rxr`, this type is primarily used for calling its `emitter_receiver` function,
/// and then you use the returned [`BehaviorSubjectEmitter`] to emit values, while
/// using [`BehaviorSubjectReceiver`] to subscribe to those values.
///
/// [`BehaviorSubjectEmitter`]: struct.BehaviorSubjectEmitter.html
/// [`BehaviorSubjectReceiver`]: struct.BehaviorSubjectReceiver.html
///
/// # Examples
///
/// `BehaviorSubject` completion
///
///```no_run
/// use rxr::{subjects::BehaviorSubject, subscribe::Subscriber};
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
/// // Initialize a `BehaviorSubject` with an initial value and obtain
/// // its emitter and receiver.
/// let (mut emitter, mut receiver) = BehaviorSubject::emitter_receiver(100);
///
/// // Registers `Subscriber` 1 and emits the default value 100 to it.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Emits 101 to registered `Subscriber` 1.
/// emitter.next(102); // Emits 102 to registered `Subscriber` 1.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2 and emits (now the default) value 102 to it.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `BehaviorSubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |_| eprintln!("Error"),
///         || println!("Completed 2"),
///     ));
///
/// // Registers `Subscriber` 3 and emits (now the default) value 102 to it.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Emits 103 to registered `Subscriber`'s 1, 2 and 3.
///
/// emitter.complete(); // Calls `complete` on registered `Subscriber`'s 1, 2 and 3.
///
/// // Subscriber 4: post-completion subscribe, completes immediately.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called post-completion, does not emit.
///```
///
/// `BehaviorSubject` error
///
///```no_run
/// use std::error::Error;
/// use std::fmt::Display;
/// use std::sync::Arc;
///
/// use rxr::{subjects::BehaviorSubject, subscribe::Subscriber};
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
/// struct BehaviorSubjectError(String);
///
/// impl Display for BehaviorSubjectError {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.0)
///     }
/// }
///
/// impl Error for BehaviorSubjectError {}
///
/// // Initialize a `BehaviorSubject` with an initial value and obtain
/// // its emitter and receiver.
/// let (mut emitter, mut receiver) = BehaviorSubject::emitter_receiver(100);
///
/// // Registers `Subscriber` 1 and emits the default value 100 to it.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Emits 101 to registered `Subscriber` 1.
/// emitter.next(102); // Emits 102 to registered `Subscriber` 1.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2 and emits (now the default) value 102 to it.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `BehaviorSubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |e| eprintln!("Error: {} 2", e),
///         || println!("Completed"),
///     ));
///
/// // Registers `Subscriber` 3 and emits (now the default) value 102 to it.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Emits 103 to registered `Subscriber`'s 1, 2 and 3.
///
/// // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
/// emitter.error(Arc::new(BehaviorSubjectError(
///     "BehaviorSubject error".to_string(),
/// )));
///
/// // Subscriber 4: subscribed after subject's error call; emits error and
/// // does not emit further.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called after subject's error call, does not emit.
///```
pub struct BehaviorSubject<T> {
    value: T,
    observers: Vec<(u64, Subscriber<T>)>,
    // fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: Send + Sync + 'static> BehaviorSubject<T> {
    /// Initializes a `BehaviorSubject` with the provided initial value and returns
    /// a tuple containing a `BehaviorSubjectEmitter` for emitting values and a
    /// `BehaviorSubjectReceiver` for subscribing to emitted values.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rxr::subjects::{BehaviorSubject, BehaviorSubjectEmitter, BehaviorSubjectReceiver};
    ///
    /// let (emitter, receiver) = BehaviorSubject::emitter_receiver(42);
    ///
    /// // Now you can use `emitter` to emit values and `receiver` to subscribe to them.
    /// ```
    #[must_use]
    pub fn emitter_receiver(value: T) -> (BehaviorSubjectEmitter<T>, BehaviorSubjectReceiver<T>) {
        let s = Arc::new(Mutex::new(BehaviorSubject {
            value,
            observers: Vec::with_capacity(15),
            // fused: false,
            completed: false,
            closed: false,
            error: None,
        }));

        (
            BehaviorSubjectEmitter(Arc::clone(&s)),
            BehaviorSubjectReceiver(Arc::clone(&s)),
        )
    }
}

/// Subscription handler for `BehaviorSubject`.
///
/// `BehaviorSubjectReceiver` acts as an `Observable`, allowing you to utilize its
/// `subscribe` method for receiving emissions from the `BehaviorSubject`'s multicasting.
/// You can also employ its `unsubscribe` method to close the `BehaviorSubject` and
/// remove registered observers.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct BehaviorSubjectReceiver<T>(Arc<Mutex<BehaviorSubject<T>>>);

/// Multicasting emitter for `BehaviorSubject`.
///
/// `BehaviorSubjectEmitter` acts as an `Observer`, allowing you to utilize its `next`,
/// `error`, and `complete` methods for multicasting emissions to all registered
/// observers within the `BehaviorSubject`.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct BehaviorSubjectEmitter<T>(Arc<Mutex<BehaviorSubject<T>>>);

impl<T> BehaviorSubjectReceiver<T> {
    /// Returns the number of registered observers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().observers.len()
    }

    /// Returns `true` if no observers are registered, `false` otherwise.
    #[must_use]
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

impl<T> crate::subscription::subscribe::Fuse for BehaviorSubjectReceiver<T> {}

impl<T: Clone + Send + Sync + 'static> Subscribeable for BehaviorSubjectReceiver<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        let key: u64 = super::gen_key().next().unwrap_or(super::random_seed());

        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return Subscription::subject_subscription(
                    UnsubscribeLogic::Nil,
                    SubscriptionHandle::Nil,
                );
            }
            // if src.fused {
            //     v.set_fused(true);
            // }
            // If BehaviorSubject is completed do not register new Subscriber.
            if src.completed {
                if let Some(err) = &src.error {
                    // BehaviorSubject completed with error. Call error() on
                    // every subsequent Subscriber.
                    v.error(Arc::clone(err));
                } else {
                    // BehaviorSubject completed. Call complete() on
                    // every subsequent Subscriber.
                    v.complete();
                }
                return Subscription::subject_subscription(
                    UnsubscribeLogic::Nil,
                    SubscriptionHandle::Nil,
                );
            }
            // Subscriber emits stored value right away when being registered unless
            // BehaviorSubject invoked complete(), error() or unsubscribe().
            v.next(src.value.clone());
            src.observers.push((key, v));
        } else {
            return Subscription::subject_subscription(
                UnsubscribeLogic::Nil,
                SubscriptionHandle::Nil,
            );
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::subject_subscription(
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

impl<T> Unsubscribeable for BehaviorSubjectReceiver<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for BehaviorSubjectEmitter<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            // Store new value in BehaviorSubject.
            src.value = v.clone();
        } else {
            return;
        }
        for (_, o) in &mut self.0.lock().unwrap().observers {
            o.next(v.clone());
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
            for (_, o) in &mut src.observers {
                o.complete();
            }
            src.completed = true;
            src.observers.clear();
        }
    }
}

impl<T: Clone + Send + 'static> From<BehaviorSubjectEmitter<T>> for Subscriber<T> {
    fn from(mut value: BehaviorSubjectEmitter<T>) -> Self {
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

impl<T: Clone + Send + Sync + 'static> From<BehaviorSubjectReceiver<T>> for Observable<T> {
    fn from(mut value: BehaviorSubjectReceiver<T>) -> Self {
        Observable::new(move |subscriber| value.subscribe(subscriber))
    }
}
