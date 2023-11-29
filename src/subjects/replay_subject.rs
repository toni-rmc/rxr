use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::{
    observer::Observer,
    subscribe::Unsubscribeable,
    subscription::subscribe::{
        Subscribeable, Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic,
    },
    Observable,
};

struct EmittedValueEntry<T>(T, Instant);

impl<T> EmittedValueEntry<T> {
    pub fn new(v: T) -> Self {
        EmittedValueEntry(v, Instant::now())
    }

    pub fn is_fresh(&self, window_size_ms: u128) -> bool {
        self.1.elapsed().as_millis() <= window_size_ms
    }
}
/// Specifies the buffer size for replaying previous emissions in `ReplaySubject`
/// when using either [`emitter_receiver`] or [`emitter_receiver_time_aware`].
///
/// [`emitter_receiver`]: struct.ReplaySubject.html#method.emitter_receiver
/// [`emitter_receiver_time_aware`]: struct.ReplaySubject.html#method.emitter_receiver_time_aware
pub enum BufSize {
    /// Specifies an infinite buffer size, allowing all emitted values to be replayed.
    Unbounded,

    /// Specifies a limited buffer size with the maximum number of values to be replayed.
    Bounded(usize),
}

/// Replaying old values to new subscribers, this variant of `Subject` emits these
/// values upon subscription.
///
/// This specialized variant of a `Subject` maintains a cache of previous values from
/// the source observable and transmits them to new subscribers upon subscription.
/// `ReplaySubject` emits all cached values before emitting new source observable
/// items.
///
/// Similar to a `BehaviorSubject`, a `ReplaySubject` can emit cached values to new
/// subscribers. However, unlike a `BehaviorSubject` that holds a single current
/// value, a `ReplaySubject` can record and replay an entire sequence of values.
///
/// Even when in a stopped state due to completion or an error in the source
/// observable, `ReplaySubject` replays cached values before notifying new subsequent
/// subscriptions of completion or an error.
///
/// When creating a `ReplaySubject`, you have the option to set the buffer size and
/// the duration to retain a value in the buffer.
///
/// In `rxr`, `ReplaySubject` offers two primary functions: `emitter_receiver`,
/// allowing specification of a buffer size for replaying previous emissions, and
/// `emitter_receiver_time_aware`, extending functionality by enabling you to set
/// both the buffer size and a time duration for values to remain in the buffer
/// before removal. Both return a tuple containing a [`ReplaySubjectEmitter`] for
/// emitting values and a [`ReplaySubjectReceiver`] for subscribing to emitted values.
///
/// [`ReplaySubjectEmitter`]: struct.ReplaySubjectEmitter.html
/// [`ReplaySubjectReceiver`]: struct.ReplaySubjectReceiver.html
///
/// # Examples
///
/// ReplaySubject completion
///
///```no_run
/// use rxr::{
///     subjects::{BufSize, ReplaySubject},
///     subscribe::Subscriber,
/// };
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
/// // Initialize a `ReplaySubject` with an unbounded buffer size and obtain
/// // its emitter and receiver.
/// let (mut emitter, mut receiver) = ReplaySubject::emitter_receiver(BufSize::Unbounded);
///
/// // Registers `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Stores 101 and emits it to registered `Subscriber` 1.
/// emitter.next(102); // Stores 102 and emits it to registered `Subscriber` 1.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2 and emits buffered values (101, 102) to it.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `ReplaySubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |_| eprintln!("Error"),
///         || println!("Completed 2"),
///     ));
///
/// // Registers `Subscriber` 3 and emits buffered values (101, 102) to it.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Stores 103 and emits it to registered `Subscriber`'s 1, 2 and 3.
///
/// emitter.complete(); // Calls `complete` on registered `Subscriber`'s 1, 2 and 3.
///
/// // Subscriber 4: post-completion subscribe, emits buffered values (101, 102, 103)
/// // and completes.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called post-completion, does not emit.
///```
///
/// ReplaySubject error
///
///```no_run
/// use std::{error::Error, fmt::Display, sync::Arc};
///
/// use rxr::{
///     subjects::{BufSize, ReplaySubject},
///     subscribe::Subscriber,
///     Unsubscribeable,
/// };
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
/// struct ReplaySubjectError(String);
///
/// impl Display for ReplaySubjectError {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.0)
///     }
/// }
///
/// impl Error for ReplaySubjectError {}
///
/// // Initialize a `ReplaySubject` with an unbounded buffer size and obtain
/// // its emitter and receiver.
/// let (mut emitter, mut receiver) = ReplaySubject::emitter_receiver(BufSize::Unbounded);
///
/// // Registers `Subscriber` 1.
/// receiver.subscribe(create_subscriber(1));
///
/// emitter.next(101); // Stores 101 and emits it to registered `Subscriber` 1.
/// emitter.next(102); // Stores 102 and emits it to registered `Subscriber` 1.
///
/// // All Observable operators can be applied to the receiver.
/// // Registers mapped `Subscriber` 2 and emits buffered values (101, 102) to it.
/// receiver
///     .clone() // Shallow clone: clones only the pointer to the `ReplaySubject`.
///     .map(|v| format!("mapped {}", v))
///     .subscribe(Subscriber::new(
///         move |v| println!("Subscriber #2 emitted: {}", v),
///         |e| eprintln!("Error: {} 2", e),
///         || println!("Completed"),
///     ));
///
/// // Registers `Subscriber` 3 and emits buffered values (101, 102) to it.
/// receiver.subscribe(create_subscriber(3));
///
/// emitter.next(103); // Stores 103 and emits it to registered `Subscriber`'s 1, 2 and 3.
///
/// // Calls `error` on registered `Subscriber`'s 1, 2 and 3.
/// emitter.error(Arc::new(ReplaySubjectError(
///     "ReplaySubject error".to_string(),
/// )));
///
/// // Subscriber 4: post-error subscribe, emits buffered values (101, 102, 103)
/// // and emits error.
/// receiver.subscribe(create_subscriber(4));
///
/// emitter.next(104); // Called post-error, does not emit.
///
/// // Closes receiver and clears registered subscribers.
/// receiver.unsubscribe();
///```
pub struct ReplaySubject<T> {
    buf_size: BufSize,
    window_size: Option<u128>,
    values: VecDeque<EmittedValueEntry<T>>,
    observers: Vec<(u64, Subscriber<T>)>,
    // fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: Send + Sync + 'static> ReplaySubject<T> {
    /// Creates a `ReplaySubject` with a specified buffer size, allowing for replaying
    /// previous emissions to new subscribers.
    ///
    /// The `buf_size` parameter determines the size of the buffer used for replaying
    /// values to new subscribers. A buffer size of `BufSize::Unbounded` means an
    /// infinite buffer, retaining all past values for replay.
    ///
    /// Returns a tuple containing a `ReplaySubjectEmitter` for emitting values and
    /// a `ReplaySubjectReceiver` for subscribing to emitted values.
    pub fn emitter_receiver(
        buf_size: BufSize,
    ) -> (ReplaySubjectEmitter<T>, ReplaySubjectReceiver<T>) {
        let mut s = ReplaySubject {
            buf_size,
            window_size: None,
            values: VecDeque::new(),
            observers: Vec::with_capacity(16),
            // fused: false,
            completed: false,
            closed: false,
            error: None,
        };

        match s.buf_size {
            BufSize::Unbounded => s.values = VecDeque::with_capacity(16),
            BufSize::Bounded(size) => s.values = VecDeque::with_capacity(size),
        }
        let s = Arc::new(Mutex::new(s));

        (
            ReplaySubjectEmitter(Arc::clone(&s)),
            ReplaySubjectReceiver(Arc::clone(&s)),
        )
    }

    /// Creates a `ReplaySubject` with a buffer to store emitted values and a
    /// time-aware window for controlling how long values stay in the buffer.
    ///
    /// The `buf_size` parameter specifies the maximum number of values to keep in
    /// the buffer. If set to `BufSize::Unbounded`, the buffer can grow indefinitely.
    /// The `window_size_ms` parameter defines the duration (in milliseconds) for
    /// which values remain in the buffer. Once this duration elapses, values are
    /// removed from the buffer.
    ///
    /// Returns a tuple containing a `ReplaySubjectEmitter` for emitting values and
    /// a `ReplaySubjectReceiver` for subscribing to emitted values.
    pub fn emitter_receiver_time_aware(
        buf_size: BufSize,
        window_size_ms: u128,
    ) -> (ReplaySubjectEmitter<T>, ReplaySubjectReceiver<T>) {
        let mut s = ReplaySubject {
            buf_size,
            window_size: Some(window_size_ms),
            values: VecDeque::new(),
            observers: Vec::with_capacity(16),
            // fused: false,
            completed: false,
            closed: false,
            error: None,
        };

        match s.buf_size {
            BufSize::Unbounded => s.values = VecDeque::with_capacity(16),
            BufSize::Bounded(size) => s.values = VecDeque::with_capacity(size),
        }
        let s = Arc::new(Mutex::new(s));

        (
            ReplaySubjectEmitter(Arc::clone(&s)),
            ReplaySubjectReceiver(Arc::clone(&s)),
        )
    }
}

/// Subscription handler for `ReplaySubject`.
///
/// `ReplaySubjectReceiver` acts as an `Observable`, allowing you to utilize its
/// `subscribe` method for receiving emissions from the `ReplaySubject`'s multicasting.
/// You can also employ its `unsubscribe` method to close the `ReplaySubject` and
/// remove registered observers.
#[derive(Clone)]
pub struct ReplaySubjectReceiver<T>(Arc<Mutex<ReplaySubject<T>>>);

/// Multicasting emitter for `ReplaySubject`.
///
/// `ReplaySubjectEmitter` acts as an `Observer`, allowing you to utilize its `next`,
/// `error`, and `complete` methods for multicasting emissions to all registered
/// observers within the `ReplaySubject`.
#[derive(Clone)]
pub struct ReplaySubjectEmitter<T>(Arc<Mutex<ReplaySubject<T>>>);

impl<T> ReplaySubjectReceiver<T> {
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

impl<T> crate::subscription::subscribe::Fuse for ReplaySubjectReceiver<T> {}

impl<T: Clone + Send + Sync + 'static> Subscribeable for ReplaySubjectReceiver<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        let key: u64 = super::gen_key().next().unwrap_or(super::random_seed());

        if let Ok(mut src) = self.0.lock() {
            // If ReplaySubject is unsubscribed `closed` flag is set. When closed
            // ReplaySubject does not emit nor subscribes.
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // if src.fused {
            //     v.set_fused(true);
            // }
            // If window_size is set remove outdated stored values from buffer.
            if let Some(window_size_ms) = src.window_size {
                // Retain only fresh values in buffer.
                src.values.retain(|e| e.is_fresh(window_size_ms));
            }

            // Subscriber emits stored values right away. Values are emitted for new
            // Subscribers even if ReplaySubject called complete() or error().
            for value in &src.values {
                v.next(value.0.clone());
            }
            // If ReplaySubject is completed do not register new Subscriber.
            if src.completed {
                if let Some(err) = &src.error {
                    // ReplaySubject completed with error. Call error() on
                    // every subsequent Subscriber.
                    v.error(Arc::clone(err));
                } else {
                    // ReplaySubject completed. Call complete() on
                    // every subsequent Subscriber.
                    v.complete();
                }
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // If ReplaySubject is not completed register new Subscriber.
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

impl<T> Unsubscribeable for ReplaySubjectReceiver<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for ReplaySubjectEmitter<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            match src.buf_size {
                BufSize::Unbounded => src.values.push_back(EmittedValueEntry::new(v.clone())),
                BufSize::Bounded(buf_size) => {
                    // Check if buffer is full.
                    if src.values.len() == buf_size {
                        // If yes, remove first entry from the buffer.
                        src.values.pop_front();
                    }
                    if buf_size > 0 {
                        // Store new value in ReplaySubject at the end of the values buffer.
                        src.values.push_back(EmittedValueEntry::new(v.clone()));
                    }
                }
            };
        } else {
            return;
        }

        // Emit value to all stored Subscribers.
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

impl<T: Clone + Send + 'static> From<ReplaySubjectEmitter<T>> for Subscriber<T> {
    fn from(mut value: ReplaySubjectEmitter<T>) -> Self {
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

impl<T: Clone + Send + Sync + 'static> From<ReplaySubjectReceiver<T>> for Observable<T> {
    fn from(mut value: ReplaySubjectReceiver<T>) -> Self {
        Observable::new(move |subscriber| value.subscribe(subscriber))
    }
}
