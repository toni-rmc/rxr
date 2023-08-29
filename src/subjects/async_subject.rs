use std::{
    error::Error,
    sync::{Arc, Mutex}
};

use crate::{
    observer::Observer,
    subscribe::Unsubscribeable,
    subscription::subscribe::{
        Subscribeable, Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic,
    },
};

pub struct AsyncSubject<T> {
    value: Option<T>,
    observers: Vec<Subscriber<T>>,
    fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: Send + Sync + 'static> AsyncSubject<T> {
    pub fn new() -> (AsyncSubjectTx<T>, AsyncSubjectRx<T>) {
        let s = Arc::new(Mutex::new(AsyncSubject {
            value: None,
            observers: Vec::with_capacity(15),
            fused: false,
            completed: false,
            closed: false,
            error: None,
        }));

        (
            AsyncSubjectTx(Arc::clone(&s)),
            AsyncSubjectRx(Arc::clone(&s)),
        )
    }
}

#[derive(Clone)]
pub struct AsyncSubjectRx<T>(Arc<Mutex<AsyncSubject<T>>>);

#[derive(Clone)]
pub struct AsyncSubjectTx<T>(Arc<Mutex<AsyncSubject<T>>>);

impl<T> AsyncSubjectRx<T> {
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().observers.len()
    }

    pub fn fuse(self) -> Self {
        for o in &mut self.0.lock().unwrap().observers {
            o.set_fused(true);
        }
        self
    }

    pub fn defuse(self) -> Self {
        for o in &mut self.0.lock().unwrap().observers {
            o.set_fused(false);
        }
        self
    }
}

impl<T: Clone + Send + Sync + 'static> Subscribeable for AsyncSubjectRx<T> {
    type ObsType = T;

        fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            if src.fused {
                v.set_fused(true);
            }
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
            src.observers.push(v);
        } else {
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned.lock().unwrap().observers.clear();
                // Maybe also mark AsyncSubject as completed?
            })),
            SubscriptionHandle::Nil,
        )
    }
}

impl<T> Unsubscribeable for AsyncSubjectRx<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for AsyncSubjectTx<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            // Store new value in AsyncSubject.
            src.value = Some(v.clone());
        }
    }

    fn error(&mut self, e: Arc<dyn Error + Send + Sync>) {
        if let Ok(mut src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
            for o in &mut src.observers {
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
                for observer in &mut src.observers {
                    observer.next(v.clone());
                }
            }
            for observer in &mut src.observers {
                observer.complete();
            }
            src.observers.clear();
        }
    }
}

impl<T: Clone + Send + 'static> From<AsyncSubjectTx<T>> for Subscriber<T> {
    fn from(value: AsyncSubjectTx<T>) -> Self {
        let mut vn = value.clone();
        let mut ve = value.clone();
        let mut vc = value.clone();
        Subscriber::new(
            move |v| {
                vn.next(v);
            },
            Some(move |e| ve.error(e)),
            Some(move || vc.complete()),
        )
    }
}

#[cfg(test)]
mod test {
    use std::{
        error::Error,
        sync::{Arc, Mutex},
    };

    use crate::{observer::Observer, subscribe::Subscriber, subjects::AsyncSubject, Subscribeable};

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
                    Some(move |_| {
                        // Track error() calls.
                        errors_c.lock().unwrap().push(1);
                    }),
                    Some(move || {
                        // Track complete() calls.
                        completes_c.lock().unwrap().push(1);
                    }),
                )
            };
            10
        ];
        (make_subscriber, nexts, completes, errors)
    }

    #[derive(Debug)]
    struct MyErr;

    impl std::fmt::Display for MyErr {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Ok(())
        }
    }

    impl Error for MyErr {}

    #[test]
    fn async_subject_complete() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = AsyncSubject::new();

        // Store first value in AsyncSubject.
        stx.next(1);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subscriber.
        srx.subscribe(x); // 1st

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Change stored value.
        stx.next(2);

        // Register more subscribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times on 3 registered subscribers.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete AsyncSubject.
        stx.complete();
        stx.next(7);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 3);
        assert_eq!(nexts.lock().unwrap().last(), Some(&6));
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Subscribe and emit stored value after complete.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 4th
        srx.subscribe(z); // 5th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 5);
        assert_eq!(nexts.lock().unwrap().last(), Some(&6));
        assert_eq!(completes.lock().unwrap().len(), 5);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn async_subject_complete_empty() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = AsyncSubject::new();

        // Register subscriber.
        srx.subscribe(x); // 1st

        // Register more subscribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete AsyncSubject.
        stx.complete();

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Ignore further `next()` calls.
        stx.next(7);

        // Subscribe and complete after completion without registering.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 4);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn async_subject_error() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = AsyncSubject::new();

        // Store first value in AsyncSubject.
        stx.next(1);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subscriber.
        srx.subscribe(x); // 1st

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Change stored value.
        stx.next(2);

        // Register more subscribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times on 3 registered subscribers.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Error AsyncSubject.
        stx.error(Arc::new(MyErr));
        stx.next(7);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);

        // Subscribe and emit stored value after error.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 4th
        srx.subscribe(z); // 5th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 5);
    }

    #[test]
    fn async_subject_error_empty() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = AsyncSubject::new();

        // Register subscriber.
        srx.subscribe(x); // 1st

        // Register more subscribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Error AsyncSubject.
        stx.error(Arc::new(MyErr));

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);

        // Ignore further `next()` calls.
        stx.next(7);

        // Subscribe and error after AsyncSubject error without registering.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 4);
    }
}

