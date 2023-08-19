use std::{
    any::Any, error::Error, future::Future, pin::Pin, sync::Arc,
    thread::JoinHandle as ThreadJoinHandle,
};

use tokio::task::{JoinError, JoinHandle};

use crate::observer::Observer;

pub trait Subscribeable {
    type ObsType;

    fn subscribe(&mut self, v: Subscriber<Self::ObsType>) -> Subscription;
}

pub trait Unsubscribeable {
    fn unsubscribe(self);
}

pub struct Subscriber<NextFnType> {
    next_fn: Box<dyn FnMut(NextFnType) + Send>,
    complete_fn: Option<Box<dyn FnMut() + Send + Sync>>,
    error_fn: Option<Box<dyn FnMut(Arc<dyn Error + Send + Sync>) + Send + Sync>>,
    completed: bool,
    fused: bool,
    errored: bool,
}

impl<NextFnType> Subscriber<NextFnType> {
    pub fn new(
        next_fnc: impl FnMut(NextFnType) + 'static + Send,
        error_fnc: Option<impl FnMut(Arc<dyn Error + Send + Sync>) + 'static + Send + Sync>,
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

    fn error(&mut self, observable_error: Arc<dyn Error + Send + Sync>) {
        if self.errored || (self.fused && self.completed) {
            return;
        }
        if let Some(efn) = &mut self.error_fn {
            (efn)(observable_error);
            self.errored = true;
        }
    }
}

pub enum SubscriptionHandle {
    Nil,
    JoinTask(JoinHandle<()>),
    JoinThread(ThreadJoinHandle<()>),
}

pub enum SubscriptionError {
    JoinTaskError(JoinError),
    JoinThreadError(Box<dyn Any + Send>),
}

pub struct Subscription {
    unsubscribe_logic: UnsubscribeLogic,
    pub subscription_future: SubscriptionHandle,
}

impl Subscription {
    pub fn new(
        unsubscribe_logic: UnsubscribeLogic,
        subscription_future: SubscriptionHandle,
    ) -> Self {
        Subscription {
            unsubscribe_logic,
            subscription_future,
        }
    }

    pub async fn join_thread_or_task(self) -> Result<(), Box<dyn Any + Send>> {
        match self.subscription_future {
            SubscriptionHandle::JoinTask(task_handle) => {
                let r = task_handle.await;
                return r.map_err(|e| Box::new(e) as Box<dyn Any + Send>);
            }
            SubscriptionHandle::JoinThread(thread_handle) => {
                let r = thread_handle.join();
                return r.map_err(|e| e);
            }
            SubscriptionHandle::Nil => {
                return Ok(());
            }
        }
    }

    pub fn join_thread(self) -> Result<(), Box<dyn Any + Send>> {
        match self.subscription_future {
            SubscriptionHandle::JoinThread(thread_handle) => {
                let r = thread_handle.join();
                return r.map_err(|e| e);
            }
            SubscriptionHandle::Nil => Ok(()),
            SubscriptionHandle::JoinTask(_) => {
                panic!("handle should be OS thread handle but it is tokio task handle instead")
            }
        }
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
    Logic(Box<dyn FnOnce() + Send>),
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
