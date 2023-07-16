use std::{
    error::Error,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    observer::Observer,
    subscription::subscribe::{Subscribeable, Subscriber, Subscription, UnsubscribeLogic},
};

pub struct Subject<T> {
    observers: Vec<Subscriber<T>>,
    fused: bool,
}

impl<T: 'static> Subject<T> {
    pub fn new() -> (SubjectTx<T>, SubjectRx<T>) {
        let s = Arc::new(Mutex::new(Subject {
            observers: Vec::with_capacity(15),
            fused: false,
        }));

        (
            SubjectTx {
                source: Arc::clone(&s),
            },
            SubjectRx {
                source: Arc::clone(&s),
            },
        )
    }
}

#[derive(Clone)]
pub struct SubjectRx<T> {
    source: Arc<Mutex<Subject<T>>>,
}

#[derive(Clone)]
pub struct SubjectTx<T> {
    source: Arc<Mutex<Subject<T>>>,
}

impl<T> SubjectRx<T> {
    pub fn len(&self) -> usize {
        self.source.lock().unwrap().observers.len()
    }

    pub fn fuse(self) -> Self {
        for o in &mut self.source.lock().unwrap().observers {
            o.set_fused(true);
        }
        self
    }

    pub fn defuse(self) -> Self {
        for o in &mut self.source.lock().unwrap().observers {
            o.set_fused(false);
        }
        self
    }
}

impl<T: 'static> Subscribeable for SubjectRx<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if self.source.lock().unwrap().fused {
            v.set_fused(true);
        }
        self.source.lock().unwrap().observers.push(v);

        let source_cloned = Arc::clone(&self.source);

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                source_cloned.lock().unwrap().observers.clear();
            })),
            None,
        )
    }
}

impl<T: Clone> Observer for SubjectTx<T> {
    type NextFnType = T;

    fn next(&mut self, v: Self::NextFnType) {
        for o in &mut self.source.lock().unwrap().observers {
            o.next(v.clone());
        }
    }

    fn error(&mut self, e: Rc<dyn Error>) {
        for o in &mut self.source.lock().unwrap().observers {
            o.error(e.clone()); // TODO: change error type to be cloneable
        }
    }

    fn complete(&mut self) {
        for o in &mut self.source.lock().unwrap().observers {
            o.complete();
        }
    }
}
