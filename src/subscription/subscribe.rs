use std::{error::Error, rc::Rc, future::Future, pin::Pin};

use tokio::task::JoinHandle;

use crate::observer::Observer;

pub trait Subscribeable {
    type ObsType;

    fn subscribe(&mut self, v: Subscriber<Self::ObsType>) -> Subscription;
}

pub trait Unsubscribeable {
    fn unsubscribe(self);
}

pub struct Subscriber<NextFnType> {
    next_fn: Box<dyn FnMut(NextFnType) + Send + Sync>,
    complete_fn: Option<Box<dyn FnMut() + Send + Sync>>,
    error_fn: Option<Box<dyn FnMut(Rc<dyn Error>) + Send + Sync>>,
    completed: bool,
    fused: bool,
    errored: bool,
}

impl<NextFnType> Subscriber<NextFnType> {
    pub fn new(
        next_fnc: impl FnMut(NextFnType) + 'static + Send + Sync,
        error_fnc: Option<impl FnMut(Rc<dyn Error>) + 'static + Send + Sync>,
        complete_fnc: Option<impl FnMut() + 'static + Send + Sync>,
    ) -> Self {
        let mut s = Subscriber {
            next_fn: Box::new(next_fnc),
            complete_fn: None,
            error_fn: None,
            completed: false,
            fused: false,
            errored: false,
        };

        if let Some(cfn) = complete_fnc {
            s.complete_fn = Some(Box::new(cfn));
        }
        if let Some(efn) = error_fnc {
            s.error_fn = Some(Box::new(efn));
        }
        s
    }

   pub(crate) fn set_fused(&mut self, f: bool) {
        self.fused = f;
    }
}

impl<N> Observer for Subscriber<N> {
    type NextFnType = N;
    fn next(&mut self, v: Self::NextFnType) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        (self.next_fn)(v);
    }

    fn complete(&mut self) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        if let Some(cfn) = &mut self.complete_fn {
            (cfn)();
            self.completed = true;
        }
    }

    fn error(&mut self, observable_error: Rc<dyn Error>) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        if let Some(efn) = &mut self.error_fn {
            (efn)(observable_error);
            self.errored = true;
        }
    }
}


pub struct Subscription {
    unsubscribe_logic: UnsubscribeLogic,
    pub subscription_future: Option<JoinHandle<()>>,
}

impl Subscription {
    pub fn new(
        unsubscribe_logic: UnsubscribeLogic,
        subscription_future: Option<JoinHandle<()>>,
    ) -> Self {
        Subscription {
            unsubscribe_logic,
            subscription_future,
        }
    }

    pub async fn join(self) -> Result<(), tokio::task::JoinError> {
        if let Some(jh) = self.subscription_future {
            let r = jh.await;
            return r;
        }
        Ok(())
    }
}

impl Unsubscribeable for Subscription {
    fn unsubscribe(self) {
        // match self.unsubscribe_logic {
        //     UnsubscribeLogic::Nil => (),
        //     UnsubscribeLogic::Logic(fnc) => fnc(),
        //     UnsubscribeLogic::Wrapped(subscription) => subscription.unsubscribe(),
        //     UnsubscribeLogic::Future(future) => {
        //         tokio::task::spawn(async {
        //             future.await;
        //         });
        //     },
        // }
        self.unsubscribe_logic.unsubscribe();
    }
}

// Unsubscribe logic type which is returned from user suppliied subscribe() function
// and wrapped in the Subscription struct.
pub enum UnsubscribeLogic {
    Nil,
    Wrapped(Box<Subscription>),
    Logic(Box<dyn FnOnce() + Send + Sync>),
    Future(Pin<Box<dyn Future<Output = ()> + Send + Sync>>),
}

impl UnsubscribeLogic {
    fn unsubscribe(mut self) -> Self {
        match self {
            UnsubscribeLogic::Nil => (),
            UnsubscribeLogic::Logic(fnc) => {
                fnc();
                self = Self::Nil;
            }
            UnsubscribeLogic::Wrapped(subscription) => {
                subscription.unsubscribe();
                self = Self::Nil;
            }
            UnsubscribeLogic::Future(future) => {
                tokio::task::spawn(async {
                    future.await;
                });
                self = Self::Nil;
            }
        }
        self
    }
}

