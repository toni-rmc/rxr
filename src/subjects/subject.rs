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

/// A `Subject` represents a unique variant of an `Observable` that enables
/// multicasting values to multiple `Observers`.
///
/// Unlike regular `Observables`, which are unicast (each subscribed `Observer` has
/// its independent execution of the `Observable`), `Subjects` are multicast.
///
/// If an error is encountered in the source observable, `Subject` will not emit any
/// items to future subscriptions. Instead, it will just pass along the error
/// notification from the source observable to these new subscriptions.
///
/// In `rxr`, you use the `Subject` type by invoking its `emitter_receiver` function
/// to get a [`SubjectEmitter`] for emitting values and a [`SubjectReceiver`] for
/// subscribing to emitted values.
///
/// [`SubjectEmitter`]: struct.SubjectEmitter.html
/// [`SubjectReceiver`]: struct.SubjectReceiver.html
///
/// # Examples
///
/// `Subject` completion
///
///```no_run
/// use std::fmt::Display;
///
/// use rxr::{subjects::Subject, subscribe::Subscriber};
/// use rxr::{ObservableExt, Observer, Subscribeable};
///
/// pub fn create_subscriber<T: Display>(subscriber_id: i32) -> Subscriber<T> {
///     Subscriber::new(
///         move |v| println!("Subscriber #{} emitted: {}", subscriber_id, v),
///         |_| eprintln!("Error"),
///         move || println!("Completed {}", subscriber_id),
///     )
/// }
///
/// // Initialize a `Subject` and obtain its emitter and receiver.
/// let (mut emitter, mut receiver) = Subject::emitter_receiver();
///
/// // Registers `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Emits 101 to registered `Subscriber` 1.
/// emitter.next(102); // Emits 102 to registered `Subscriber` 1.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `Subject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(create_subscriber(2));
///
/// // Registers `Subscriber` 3.
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
/// Utilizing a `Subject` as an `Observer`. This can be done with any variant of `Subject`.
///
///```no_run
/// use std::{fmt::Display, time::Duration};
///
/// use rxr::{
///     subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
///     Observable, ObservableExt, Observer, Subject, Subscribeable,
/// };
///
/// pub fn create_subscriber<T: Display>(subscriber_id: u32) -> Subscriber<T> {
///     Subscriber::new(
///         move |v: T| println!("Subscriber {}: {}", subscriber_id, v),
///         move |e| eprintln!("Error {}: {}", subscriber_id, e),
///         move || println!("Completed Subscriber {}", subscriber_id),
///     )
/// }
///
/// // Make an Observable.
/// let mut observable = Observable::new(|mut o: Subscriber<_>| {
///     for i in 0..10 + 1 {
///         o.next(i);
///         std::thread::sleep(Duration::from_millis(1));
///     }
///     o.complete();
///     Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
/// });
///
/// // Initialize a `Subject` and obtain its emitter and receiver.
/// let (emitter, mut receiver) = Subject::emitter_receiver();
///
/// // Register `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// // Register `Subscriber` 2.
/// receiver
///     // We're cloning the receiver so we can use it again.
///     // Shallow clone: clones only the pointer to the `Subject`.
///     .clone()
///     .take(7) // For performance, prioritize placing `take()` as the first operator.
///     .delay(1000)
///     .map(|v| format!("mapped {}", v))
///     .subscribe(create_subscriber(2));
///
/// // Register `Subscriber` 3.
/// receiver
///     .filter(|v| v % 2 == 0)
///     .map(|v| format!("filtered {}", v))
///     .subscribe(create_subscriber(3));
///
/// // Convert the emitter into an observer and subscribe it to the observable.
/// observable.subscribe(emitter.into());
///```
pub struct Subject<T> {
    observers: Vec<(u64, Subscriber<T>)>,
    // fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: 'static> Subject<T> {
    /// Creates a new pair of `SubjectEmitter` for emitting values and
    /// `SubjectReceiver` for subscribing to values.
    #[must_use]
    pub fn emitter_receiver() -> (SubjectEmitter<T>, SubjectReceiver<T>) {
        let s = Arc::new(Mutex::new(Subject {
            observers: Vec::with_capacity(16),
            // fused: false,
            completed: false,
            closed: false,
            error: None,
        }));

        (
            SubjectEmitter(Arc::clone(&s)),
            SubjectReceiver(Arc::clone(&s)),
        )
    }
}

/// Subscription handler for `Subject`.
///
/// `SubjectReceiver` acts as an `Observable`, allowing you to utilize its
/// `subscribe` method for receiving emissions from the `Subject`'s multicasting.
/// You can also employ its `unsubscribe` method to close the `Subject` and
/// remove registered observers.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct SubjectReceiver<T>(Arc<Mutex<Subject<T>>>);

/// Multicasting emitter for `Subject`.
///
/// `SubjectEmitter` acts as an `Observer`, allowing you to utilize its `next`,
/// `error`, and `complete` methods for multicasting emissions to all registered
/// observers within the `Subject`.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct SubjectEmitter<T>(Arc<Mutex<Subject<T>>>);

impl<T> SubjectReceiver<T> {
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

impl<T> crate::subscription::subscribe::Fuse for SubjectReceiver<T> {}

impl<T: 'static> Subscribeable for SubjectReceiver<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        let key: u64 = super::gen_key().next().unwrap_or(super::random_seed());

        if let Ok(mut src) = self.0.lock() {
            // If Subject is unsubscribed `closed` flag is set. When closed
            // Subject does not emit nor subscribes.
            if src.closed {
                return Subscription::subject_subscription(
                    UnsubscribeLogic::Nil,
                    SubscriptionHandle::Nil,
                );
            }
            // if src.fused {
            //     v.set_fused(true);
            // }
            // If Subject is completed do not register new Subscriber.
            if src.completed {
                if let Some(err) = &src.error {
                    // Subject completed with error. Call error() on
                    // every subsequent Subscriber.
                    v.error(Arc::clone(err));
                } else {
                    // Subject completed. Call complete() on
                    // every subsequent Subscriber.
                    v.complete();
                }
                return Subscription::subject_subscription(
                    UnsubscribeLogic::Nil,
                    SubscriptionHandle::Nil,
                );
            }
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

impl<T> Unsubscribeable for SubjectReceiver<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for SubjectEmitter<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
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

impl<T: Clone + 'static> From<SubjectEmitter<T>> for Subscriber<T> {
    fn from(mut value: SubjectEmitter<T>) -> Self {
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

impl<T: Clone + Send + Sync + 'static> From<SubjectReceiver<T>> for Observable<T> {
    fn from(mut value: SubjectReceiver<T>) -> Self {
        Observable::new(move |subscriber| value.subscribe(subscriber))
    }
}
