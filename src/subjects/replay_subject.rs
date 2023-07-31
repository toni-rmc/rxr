use std::{
    error::Error,
    rc::Rc,
    sync::{Arc, Mutex}, collections::VecDeque, time::Instant,
};

use crate::{
    observer::Observer,
    subscription::subscribe::{Subscribeable, Subscriber, Subscription, UnsubscribeLogic, SubscriptionHandle},
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

pub enum BufSize {
    Infinity,
    Fixed(usize)
}

pub struct ReplaySubject<T> {
    buf_size: BufSize,
    window_size: Option<u128>,
    values: VecDeque<EmittedValueEntry<T>>,
    observers: Vec<Subscriber<T>>,
    fused: bool,
    closed: bool,
}

impl<T: Send + Sync + 'static> ReplaySubject<T> {
    pub fn new(buf_size: BufSize) -> (ReplaySubjectTx<T>, ReplaySubjectRx<T>) {
        let mut s = ReplaySubject {
            buf_size,
            window_size: None,
            values: VecDeque::new(),
            observers: Vec::with_capacity(16),
            fused: false,
            closed: false,
        };

        match s.buf_size {
            BufSize::Infinity => s.values = VecDeque::with_capacity(16),
            BufSize::Fixed(size) => s.values = VecDeque::with_capacity(size),
        }
        let s = Arc::new(Mutex::new(s));

        (
            ReplaySubjectTx(Arc::clone(&s)),
            ReplaySubjectRx(Arc::clone(&s)),
        )
    }

    pub fn new_time_aware(buf_size: BufSize, window_size: u128) -> (ReplaySubjectTx<T>, ReplaySubjectRx<T>) {
        let mut s = ReplaySubject {
            buf_size,
            window_size: Some(window_size),
            values: VecDeque::new(),
            observers: Vec::with_capacity(16),
            fused: false,
            closed: false,
        };

        match s.buf_size {
            BufSize::Infinity => s.values = VecDeque::with_capacity(16),
            BufSize::Fixed(size) => s.values = VecDeque::with_capacity(size),
        }
        let s = Arc::new(Mutex::new(s));

        (
            ReplaySubjectTx(Arc::clone(&s)),
            ReplaySubjectRx(Arc::clone(&s)),
        )
    }
}

#[derive(Clone)]
pub struct ReplaySubjectRx<T>(Arc<Mutex<ReplaySubject<T>>>);

#[derive(Clone)]
pub struct ReplaySubjectTx<T>(Arc<Mutex<ReplaySubject<T>>>);

impl<T> ReplaySubjectRx<T> {
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

impl<T: Clone + Send + Sync + 'static> Subscribeable for ReplaySubjectRx<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if let Ok(mut src) = self.0.lock() {
            if src.fused {
                v.set_fused(true);
            }
            // If window_size is set remove outdated stored values from buffer.
            if let Some(window_size_ms) = src.window_size {
                // Retain only fresh values in buffer.
                println!("%%%%%%%%%%%%%%%%%%%%%%%%%%% {:?}", src.values.len());
                src.values.retain(|e| e.is_fresh(window_size_ms));
                println!("---------------- {:?}", src.values.len());
            }

            // Subscriber emits stored values right away.
            for value in &src.values {
                v.next((*value).0.clone());
            }
            // If ReplaySubject is closed do not register new Subscriber.
            if src.closed {
                return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
            }
            // If ReplaySubject is not closed register new Subscriber.
            src.observers.push(v);
        } else {
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        };

        let source_cloned = Arc::clone(&self.0);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned.lock().unwrap().observers.clear();
                // Maybe also mark ReplaySubject as closed?
            })),
            SubscriptionHandle::Nil,
        )
    }
}

impl<T: Clone> Observer for ReplaySubjectTx<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        if let Ok(mut src) = self.0.lock() {
            if src.closed {
                return;
            }
            match src.buf_size {
                BufSize::Infinity => src.values.push_back(EmittedValueEntry::new(v.clone())),
                BufSize::Fixed(buf_size) => {
                    // Check if buffer is full.
                    if src.values.len() == buf_size {
                        // If yes, remove first entry from the buffer.
                        src.values.pop_front();
                    }
                    if buf_size > 0 {
                        // Store new value in ReplaySubject at the end of the values buffer.
                        src.values.push_back(EmittedValueEntry::new(v.clone()));
                    }
                },
            };
        } else {
            return;
        }

        // Emit value to all stored Subscribers.
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

impl<T: Clone + Send + 'static> From<ReplaySubjectTx<T>> for Subscriber<T> {
    fn from(value: ReplaySubjectTx<T>) -> Self {
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
    use std::{sync::{Arc, Mutex}, error::Error, rc::Rc, time::Duration};

    use crate::{subscribe::Subscriber, observer::Observer, Subscribeable, subjects::{ReplaySubject, BufSize}};

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
    fn replay_subject_emit_than_complete() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let (mut stx, mut srx) = ReplaySubject::new(BufSize::Fixed(5));

        // Emit but no registered subsribers yet, still store emitted value.
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

        // Emit once to one registered subscriber and store emitted value.
        stx.next(2);

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit two more times to one registered subscriber and store two emitted values.
        stx.next(3);
        stx.next(4);

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 4);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register more subsribers and emit all stored values when registered.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 12);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit again on 3 registered subsribers and store emitted value.
        stx.next(5);

        // Emit again on 3 registered subsribers and store emitted value
        // but this time buffer is full so remove oldest stored value from value buffer.
        stx.next(6);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 18);
        assert_eq!(nexts.lock().unwrap().last(), Some(&6));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete ReplaySubject.
        stx.complete();

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 18);
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register another subscriber and emit some values after complete.
        let z = make_subscriber.pop().unwrap()();

        // Emit stored values upon subscribing even after complete.
        srx.subscribe(z);

        // Ignore emits after complete.
        stx.next(7);
        stx.next(8);
        stx.next(9);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 23); // 18 stored values + 5 stored values
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn replay_subject_emit_than_error() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();

        let (mut stx, mut srx) = ReplaySubject::new(BufSize::Infinity);

        // Register some subsribers.
        srx.subscribe(x);
        srx.subscribe(y);
        srx.subscribe(z);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Emit some values.
        stx.next(1);
        stx.next(2);
        stx.next(3);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 9);
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

        // Invoke error on a ReplaySubject.
        stx.error(Rc::new(MyErr));

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 9);
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

    #[test]
    fn replay_subject_emit_than_complete_time_aware() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();

        // Initialize ReplaySubject with time aware value buffer.
        let (mut stx, mut srx) = ReplaySubject::new_time_aware(BufSize::Fixed(10), 500);

        // Emit but no registered subsribers yet, still store emitted value.
        stx.next(1);
        stx.next(2);
        
        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subsriber and emit stored value.
        srx.subscribe(x); // 1st

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Wait for some time to let time aware ReplaySubject removes old values from the buffer.
        std::thread::sleep(Duration::from_millis(700));

        // Register more subsribers. No emits should occur because previosly
        // registered values are outdated.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        stx.next(3);

        std::thread::sleep(Duration::from_millis(700));
        stx.next(4);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 8);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Complete the ReplaySubject.
        stx.complete();

        // These emits after complete are ignored.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 8);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // ReplaySubject emits upon subscription even after complete.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 9);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 3);
        assert_eq!(errors.lock().unwrap().len(), 0);
    }

    #[test]
    fn replay_subject_emit_than_error_time_aware() {
        let (mut make_subscriber, nexts, completes, errors) = subject_value_registers();

        let x = make_subscriber.pop().unwrap()();

        // Initialize ReplaySubject with time aware value buffer.
        let (mut stx, mut srx) = ReplaySubject::new_time_aware(BufSize::Fixed(10), 500);

        // Emit but no registered subsribers yet, still store emitted value.
        stx.next(1);
        stx.next(2);
        
        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 0);
        assert_eq!(nexts.lock().unwrap().last(), None);
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Register subsriber and emit stored value.
        srx.subscribe(x); // 1st

        assert_eq!(srx.len(), 1);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        // Wait for some time to let time aware ReplaySubject removes old values from the buffer.
        std::thread::sleep(Duration::from_millis(700));

        // Register more subsribers. No emits should occur because previosly
        // registered values are outdated.
        let y = make_subscriber.pop().unwrap()();
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(y); // 2nd
        srx.subscribe(z); // 3rd

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 2);
        assert_eq!(nexts.lock().unwrap().last(), Some(&2));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 0);

        stx.next(3);

        std::thread::sleep(Duration::from_millis(700));
        stx.next(4);

        assert_eq!(srx.len(), 3);
        assert_eq!(nexts.lock().unwrap().len(), 8);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
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

        // Invoke error on ReplaySubject.
        stx.error(Rc::new(MyErr));

        // These emits after error are ignored.
        stx.next(5);
        stx.next(6);

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 8);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);

        // ReplaySubject emits upon subscription even after error.
        let z = make_subscriber.pop().unwrap()();
        srx.subscribe(z); // 4th

        assert_eq!(srx.len(), 0);
        assert_eq!(nexts.lock().unwrap().len(), 9);
        assert_eq!(nexts.lock().unwrap().last(), Some(&4));
        assert_eq!(completes.lock().unwrap().len(), 0);
        assert_eq!(errors.lock().unwrap().len(), 3);
    }
}

