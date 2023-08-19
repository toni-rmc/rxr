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
};

pub struct BehaviorSubject<T> {
    value: T,
    observers: Vec<Subscriber<T>>,
    fused: bool,
    completed: bool,
    closed: bool,
    error: Option<Arc<dyn Error + Send + Sync>>,
}

impl<T: Send + Sync + 'static> BehaviorSubject<T> {
    pub fn new(value: T) -> (BehaviorSubjectTx<T>, BehaviorSubjectRx<T>) {
        let s = Arc::new(Mutex::new(BehaviorSubject {
            value,
            observers: Vec::with_capacity(15),
            fused: false,
            completed: false,
            closed: false,
            error: None,
        }));

        (
            BehaviorSubjectTx(Arc::clone(&s)),
            BehaviorSubjectRx(Arc::clone(&s)),
        )
    }
}

#[derive(Clone)]
pub struct BehaviorSubjectRx<T>(Arc<Mutex<BehaviorSubject<T>>>);

#[derive(Clone)]
pub struct BehaviorSubjectTx<T>(Arc<Mutex<BehaviorSubject<T>>>);

impl<T> BehaviorSubjectRx<T> {
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

impl<T: Clone + Send + Sync + 'static> Subscribeable for BehaviorSubjectRx<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            if src.fused {
                v.set_fused(true);
            }
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
            src.observers.push(v);
        } else {
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned.lock().unwrap().observers.clear();
                // Maybe also mark BehaviorSubject as completed?
            })),
            SubscriptionHandle::Nil,
        )
    }
}

impl<T> Unsubscribeable for BehaviorSubjectRx<T> {
    fn unsubscribe(self) {
        if let Ok(mut r) = self.0.lock() {
            r.closed = true;
            r.observers.clear();
        }
    }
}

impl<T: Clone> Observer for BehaviorSubjectTx<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(src) = self.0.lock() {
            if src.completed || src.closed {
                return;
            }
        } else {
            return;
        }
        // Store new value in BehaviorSubject.
        self.0.lock().unwrap().value = v.clone();
        for o in &mut self.0.lock().unwrap().observers {
            o.next(v.clone());
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
            for o in &mut src.observers {
                o.complete();
            }
            src.completed = true;
            src.observers.clear();
        }
    }
}

impl<T: Clone + Send + 'static> From<BehaviorSubjectTx<T>> for Subscriber<T> {
    fn from(value: BehaviorSubjectTx<T>) -> Self {
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

    #[test]
    fn behavior_subject_emit_than_complete() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = BehaviorSubject::new(9);

        // Emit but no registered subsribers yet.
        stx.next(1);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subsriber and emit stored value.
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

        // Register more subsribers.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 6);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times on 3 registered subsribers.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(nexts.lock().unwrap().last(), Some(&6));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete Subject.
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

        let (mut stx, mut srx) = BehaviorSubject::new(1);

        // Register some subsribers and emit stored values.
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

        // Invoke error on a Subject.
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
