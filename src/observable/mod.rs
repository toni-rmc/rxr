use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::subscription::subscribe::{
    Subscribeable, Subscriber, Subscription, SubscriptionHandle, Unsubscribeable,
};
use crate::{observer::Observer, subscription::subscribe::UnsubscribeLogic};

pub struct Observable<T> {
    subscribe_fn: Box<dyn FnMut(Subscriber<T>) -> Subscription + Send + Sync>,
    fused: bool,
}

impl<T> Observable<T> {
    pub fn new(sf: impl FnMut(Subscriber<T>) -> Subscription + Send + Sync + 'static) -> Self {
        Observable {
            subscribe_fn: Box::new(sf),
            fused: false,
        }
    }

    pub fn fuse(mut self) -> Self {
        self.fused = true;
        self
    }

    pub fn defuse(mut self) -> Self {
        self.fused = false;
        self
    }
}

pub trait ObservableExt<T: 'static>: Subscribeable<ObsType = T> {
    fn map<U, F>(mut self, f: F) -> Observable<U>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnOnce(T) -> U) + Copy + Sync + Send + 'static,
        U: 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    let t = f(v);
                    o_shared.lock().unwrap().next(t);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn filter<P>(mut self, predicate: P) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
        P: (FnOnce(&T) -> bool) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    if predicate(&v) {
                        o_shared.lock().unwrap().next(v);
                    }
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn skip(mut self, n: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let mut n = n;
            let u = Subscriber::new(
                move |v| {
                    if n > 0 {
                        n -= 1;
                        return;
                    }
                    o_shared.lock().unwrap().next(v);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn delay(mut self, num_of_ms: u64) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    std::thread::sleep(Duration::from_millis(num_of_ms));
                    o_shared.lock().unwrap().next(v);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn take(mut self, n: usize) -> Observable<T>
    where
        Self: Sized + Send + Sync + 'static,
    {
        let mut i = 0;

        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let (tx, rx) = std::sync::mpsc::channel();
            let mut signal_sent = false;

            let u = Subscriber::new(
                move |v| {
                    if i < n {
                        i += 1;
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        let tx = tx.clone();
                        signal_sent = true;
                        // std::thread::spawn(move || {
                        // println!("Send >>>>>>>>>>>>>>>");
                        let _ = tx.send(true);
                        // });
                    }
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );

            let mut unsubscriber = self.subscribe(u);
            let ijh = unsubscriber.subscription_future;
            unsubscriber.subscription_future = SubscriptionHandle::Nil;

            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            let _ = std::thread::spawn(move || {
                if let Ok(_) = rx.recv() {
                    // println!("SIGNAL received");
                    if let Some(s) = unsubscriber.lock().unwrap().take() {
                        // println!("UNSUBSCRIBE called");
                        s.unsubscribe();
                    }
                }
                // println!("RECEIVER dropped in take()");
            });

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    if let Some(s) = u_cloned.lock().unwrap().take() {
                        s.unsubscribe();
                    }
                })),
                ijh,
            )
        })
    }

    fn switch_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let mut current_subscription: Option<Subscription> = None;

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                        }),
                    );
                    let s = inner_observable.subscribe(inner_subscriber);

                    if let Some(subscription) = current_subscription.take() {
                        subscription.unsubscribe();
                    }
                    current_subscription = Some(s);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn merge_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                        }),
                    );
                    inner_observable.subscribe(inner_subscriber);
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn concat_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let pending_observables: Arc<Mutex<VecDeque<(Observable<R>, Subscriber<R>)>>> =
                Arc::new(Mutex::new(VecDeque::new()));

            let mut first_pass = true;

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);
                    let po_cloned = Arc::clone(&pending_observables);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();
                            if let Some((mut io, is)) = po_cloned.lock().unwrap().pop_front() {
                                io.subscribe(is);
                            }
                        }),
                    );

                    if first_pass {
                        inner_observable.subscribe(inner_subscriber);
                        first_pass = false;
                        return;
                    }
                    pending_observables
                        .lock()
                        .unwrap()
                        .push_back((inner_observable, inner_subscriber));
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    fn exhaust_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
        Self: Sized + Send + Sync + 'static,
        F: (FnMut(T) -> Observable<R>) + Sync + Send + 'static,
    {
        let project = Arc::new(Mutex::new(project));
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let active_subscription = Arc::new(Mutex::new(false));
            let guard = Arc::new(Mutex::new(true));

            let u = Subscriber::new(
                move |v| {
                    let as_cloned = Arc::clone(&active_subscription);
                    let as_cloned2 = Arc::clone(&active_subscription);
                    let project = Arc::clone(&project);

                    let _guard = guard.lock().unwrap();

                    // Check if previous subscription completed.
                    let is_previous_subscription_active = *as_cloned.lock().unwrap();

                    // if hn {
                    //     println!("TRY TO SEND ??????????????????????????????");
                    //     return;
                    // }
                    // else {
                    //     println!("SENT!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                    //     *as_cloned.lock().unwrap() = true;
                    // }

                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned_e = Arc::clone(&o_shared);
                    let o_cloned_c = Arc::clone(&o_shared);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        Some(move |observable_error| {
                            o_cloned_e.lock().unwrap().error(observable_error);
                        }),
                        Some(move || {
                            o_cloned_c.lock().unwrap().complete();

                            // Mark this inner subscription as completed so that next
                            // one can be allowed to emit all of it's values.
                            *as_cloned2.lock().unwrap() = false;
                        }),
                    );

                    // Do not subscribe current inner subscription if previous one is active.
                    if !is_previous_subscription_active {
                        // tokio::task::spawn(async move {
                        // Mark this inner subscription as active so other following
                        // subscriptions are rejected until this one completes.
                        *as_cloned.lock().unwrap() = true;
                        inner_observable.subscribe(inner_subscriber);
                        // });
                    }
                },
                Some(move |observable_error| {
                    o_cloned_e.lock().unwrap().error(observable_error);
                }),
                Some(move || {
                    o_cloned_c.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }
}

impl<T: 'static> Subscribeable for Observable<T> {
    type ObsType = T;

    fn subscribe(&mut self, mut v: Subscriber<Self::ObsType>) -> Subscription {
        if self.fused {
            v.set_fused(true);
        }
        (self.subscribe_fn)(v)
    }
}

impl<O, T: 'static> ObservableExt<T> for O where O: Subscribeable<ObsType = T> {}

#[cfg(test)]
mod tests;
