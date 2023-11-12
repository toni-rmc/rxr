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
/// BehaviorSubject completion
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
/// BehaviorSubject error
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
#[derive(Clone)]
pub struct BehaviorSubjectReceiver<T>(Arc<Mutex<BehaviorSubject<T>>>);

/// Multicasting emitter for `BehaviorSubject`.
///
/// `BehaviorSubjectEmitter` acts as an `Observer`, allowing you to utilize its `next`,
/// `error`, and `complete` methods for multicasting emissions to all registered
/// observers within the `BehaviorSubject`.
#[derive(Clone)]
pub struct BehaviorSubjectEmitter<T>(Arc<Mutex<BehaviorSubject<T>>>);

impl<T> BehaviorSubjectReceiver<T> {
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().observers.len()
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

impl<T: Clone + Send + Sync + 'static> Subscribeable for BehaviorSubjectReceiver<T> {
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
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // Subscriber emits stored value right away when being registered unless
            // BehaviorSubject invoked complete(), error() or unsubscribe().
            v.next(src.value.clone());
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

#[cfg(test)]
mod test {
    use std::{
        error::Error,
        sync::{Arc, Mutex},
    };

    use crate::{
        observer::Observer, subjects::BehaviorSubject, subscribe::Subscriber, Subscribeable,
    };

    fn subject_value_registers() -> (
        Vec<impl FnOnce() -> Subscriber<i32>>,
        Arc<Mutex<Vec<i32>>>,
        Arc<Mutex<Vec<i32>>>,
        Arc<Mutex<Vec<i32>>>,
    ) {
        let nexts: Vec<i32> = Vec::with_capacity(5);
        let nexts = Arc::new(Mutex::new(nexts));
        let nexts_c = Arc::clone(&nexts);

        let completes: Vec<i32> = Vec::with_capacity(5);
        let completes = Arc::new(Mutex::new(completes));
        let completes_c = Arc::clone(&completes);

        let errors: Vec<i32> = Vec::with_capacity(5);
        let errors = Arc::new(Mutex::new(errors));
        let errors_c = Arc::clone(&errors);

        let make_subscriber = vec![
            move || {
                Subscriber::new(
                    move |n| {
                        // Track next() calls.
                        nexts_c.lock().unwrap().push(n);
                    },
                    move |_| {
                        // Track error() calls.
                        errors_c.lock().unwrap().push(1);
                    },
                    move || {
                        // Track complete() calls.
                        completes_c.lock().unwrap().push(1);
                    },
                )
            };
            10
        ];
        (make_subscriber, nexts, completes, errors)
    }

    #[test]
    fn behavior_subject_emit_than_complete() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = BehaviorSubject::emitter_receiver(9);

        // Emit but no registered subscribers yet.
        stx.next(1);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subscriber and emit stored value.
        srx.subscribe(x); // 1st

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 1);
        assert_eq!(nexts.lock().unwrap().last(), Some(&1));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit once to one registered subscriber.
        stx.next(2);

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times to one registered subscriber.
        stx.next(3);
        stx.next(4);

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 4);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register more subscribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 6);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times on 3 registered subscribers.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(nexts.lock().unwrap().last(), Some(&6));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete BehaviorSubject.
        stx.complete();

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register another subscriber and emit some values after complete.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th
        stx.next(7);
        stx.next(8);
        stx.next(9);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 4);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn behaviour_subject_emit_than_error() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();

        let (mut stx, mut srx) = BehaviorSubject::emitter_receiver(1);

        // Register some subscribers and emit stored values.
        srx.subscribe(x); // 1st
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 3);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit some values.
        stx.next(1);
        stx.next(2);
        stx.next(3);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        #[derive(Debug)]
        struct MyErr;

        impl std::fmt::Display for MyErr {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                Ok(())
            }
        }

        impl Error for MyErr {}

        // Invoke error on a BehaviorSubject.
        stx.error(Arc::new(MyErr));

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);

        // Register another subscriber and emit some values after error.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th
        stx.next(4);
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 4);
    }
}
