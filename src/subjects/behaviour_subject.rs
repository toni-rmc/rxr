use std::{
    error::Error,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    observer::Observer,
    subscription::subscribe::{Subscribeable, Subscriber, Subscription, UnsubscribeLogic, SubscriptionHandle},
};

pub struct BehaviourSubject<T> {
    value: T,
    observers: Vec<Subscriber<T>>,
    fused: bool,
    closed: bool,
}

impl<T: Send + Sync + 'static> BehaviourSubject<T> {
    pub fn new(value: T) -> (BehaviourSubjectTx<T>, BehaviourSubjectRx<T>) {
        let s = Arc::new(Mutex::new(BehaviourSubject {
            value,
            observers: Vec::with_capacity(15),
            fused: false,
            closed: false,
        }));

        (
            BehaviourSubjectTx(Arc::clone(&s)),
            BehaviourSubjectRx(Arc::clone(&s)),
        )
    }
}

#[derive(Clone)]
pub struct BehaviourSubjectRx<T>(Arc<Mutex<BehaviourSubject<T>>>);

#[derive(Clone)]
pub struct BehaviourSubjectTx<T>(Arc<Mutex<BehaviourSubject<T>>>);

impl<T> BehaviourSubjectRx<T> {
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

impl<T: Clone + Send + Sync + 'static> Subscribeable for BehaviourSubjectRx<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            if src.fused {
                v.set_fused(true);
            }
            // Subscriber emits stored value right away when being registered.
            v.next(src.value.clone());
            src.observers.push(v);
        } else {
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned.lock().unwrap().observers.clear();
                // Maybe also mark BehaviourSubject as closed?
            })),
            SubscriptionHandle::Nil,
        )
    }
}

impl<T: Clone> Observer for BehaviourSubjectTx<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if self.0.lock().unwrap().closed {
            return;
        }
        // Store new value in BehaviourSubject.
        self.0.lock().unwrap().value = v.clone();
        for o in &mut self.0.lock().unwrap().observers {
            o.next(v.clone());
        }
    }

    fn error(&mut self, e: Rc<dyn Error>) {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return;
            }
            for o in &mut src.observers {
                o.error(e.clone());
            }
            src.closed = true;
            src.observers.clear();
        }
    }

    fn complete(&mut self) {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return;
            }
            for o in &mut src.observers {
                o.complete();
            }
            src.closed = true;
            src.observers.clear();
        }
    }
}

impl<T: Clone + Send + 'static> From<BehaviourSubjectTx<T>> for Subscriber<T> {
    fn from(value: BehaviourSubjectTx<T>) -> Self {
        let mut vn = value.clone();
        let mut ve = value.clone();
        let mut vc = value.clone();
        Subscriber::new(
            move |v| {
                println!("IN FROM REPLAYSUBJECTTX {} ", Arc::strong_count(&vn.0));
                vn.next(v);
            },
            Some(move |e| {
                ve.error(e)
            }),
            Some(move || {
                println!("IN FROM COMPLETE REPLAYSUBJECTTX");
                vc.complete()
            })
        )
    }
}

#[cfg(test)]
mod test {
    use std::{sync::{Arc, Mutex}, error::Error, rc::Rc};

    use crate::{subscribe::Subscriber, observer::Observer, Subscribeable, subjects::BehaviourSubject};

    fn subject_value_registers() -> (
        Vec<impl FnOnce() -> Subscriber<i32>>,
        Arc<Mutex<Vec<i32>>>,
        Arc<Mutex<Vec<i32>>>,
        Arc<Mutex<Vec<i32>>>) {

        let nexts: Vec<i32> = Vec::with_capacity(5);
        let nexts = Arc::new(Mutex::new(nexts));
        let nexts_c = Arc::clone(&nexts);

        let completes: Vec<i32> = Vec::with_capacity(5);
        let completes = Arc::new(Mutex::new(completes));
        let completes_c = Arc::clone(&completes);

        let errors: Vec<i32> = Vec::with_capacity(5);
        let errors = Arc::new(Mutex::new(errors));
        let errors_c = Arc::clone(&errors);

        let make_subscriber = vec![move || {
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
                })
        )}; 10];
        (make_subscriber, nexts, completes, errors)
    }

    #[test]
    fn behaviour_subject_emit_than_complete() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = BehaviourSubject::new(9);

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
        srx.subscribe(z);
        stx.next(7);
        stx.next(8);
        stx.next(9);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn behaviour_subject_emit_than_error() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();

        let (mut stx, mut srx) = BehaviourSubject::new(1);

        // Register some subsribers and emit stored values.
        srx.subscribe(x);
        srx.subscribe(y);
        srx.subscribe(z);

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
        stx.error(Rc::new(MyErr));

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);

        // Register another subscriber and emit some values after error.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z);
        stx.next(4);
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);
    }
}
