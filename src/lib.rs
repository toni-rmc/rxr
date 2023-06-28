#![allow(dead_code, unused_variables)]

mod errors;

pub use errors::*;

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
    },
    time::Duration,
};

use tokio::task::JoinHandle;

pub trait Observer {
    type NextFnType;

    fn next(&mut self, _: Self::NextFnType);
    fn complete(&mut self);
    fn error(&mut self, _: ObservableError);
}

pub trait Subscribeable {
    type ObsType;

    fn subscribe(&mut self, v: Subscriber<Self::ObsType>) -> Subscription;
}

pub trait Unsubscribeable {
    fn unsubscribe(self);
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

pub struct Subscriber<NextFnType> {
    next_fn: Box<dyn FnMut(NextFnType) + Send + Sync>,
    complete_fn: Option<Box<dyn FnMut() + Send + Sync>>,
    error_fn: Option<Box<dyn FnMut(ObservableError) + Send + Sync>>,
}

impl<NextFnType> Subscriber<NextFnType> {
    pub fn new(
        next_fnc: impl FnMut(NextFnType) + 'static + Send + Sync,
        error_fnc: Option<impl FnMut(ObservableError) + 'static + Send + Sync>,
        complete_fnc: Option<impl FnMut() + 'static + Send + Sync>,
    ) -> Self {
        let mut s = Subscriber {
            next_fn: Box::new(next_fnc),
            complete_fn: None,
            error_fn: None,
        };

        if let Some(cfn) = complete_fnc {
            s.complete_fn = Some(Box::new(cfn));
        }
        if let Some(efn) = error_fnc {
            s.error_fn = Some(Box::new(efn));
        }
        s
    }
}

impl<N> Observer for Subscriber<N> {
    type NextFnType = N;
    fn next(&mut self, v: Self::NextFnType) {
        (self.next_fn)(v);
    }

    fn complete(&mut self) {
        if let Some(cfn) = &mut self.complete_fn {
            (cfn)();
        }
    }

    fn error(&mut self, observable_error: ObservableError) {
        if let Some(efn) = &mut self.error_fn {
            (efn)(observable_error);
        }
    }
}

pub struct Observable<T> {
    subscribe_fn: Box<dyn FnMut(Subscriber<T>) -> Subscription + Send + Sync>,
}

impl<T: 'static> Observable<T> {
    pub fn new(sf: impl FnMut(Subscriber<T>) -> Subscription + Send + Sync + 'static) -> Self {
        Observable {
            subscribe_fn: Box::new(sf),
        }
    }

    pub fn map<U, F>(mut self, f: F) -> Observable<U>
    where
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

    pub fn filter<P>(mut self, predicate: P) -> Observable<T>
    where
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

    pub fn skip (mut self, n: usize) -> Observable<T> {
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

    pub fn delay(mut self, num_of_ms: u64) -> Observable<T> {
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

    pub fn take(mut self, n: usize) -> Observable<T> {
        let mut i = 0;

        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned_e = Arc::clone(&o_shared);
            let o_cloned_c = Arc::clone(&o_shared);

            let (tx, mut rx) = tokio::sync::mpsc::channel(10);
            let mut signal_sent = false;

            let u = Subscriber::new(
                move |v| {
                    if i < n {
                        i += 1;
                        o_shared.lock().unwrap().next(v);
                    } else if !signal_sent {
                        let tx = tx.clone();
                        signal_sent = true;
                        tokio::task::spawn(async move {
                            // println!("Send >>>>>>>>>>>>>>>");
                            tx.send(true).await.unwrap();
                        });
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
            let ijh = unsubscriber.subscription_future.take();

            let unsubscriber = Arc::new(Mutex::new(Some(unsubscriber)));
            let u_cloned = Arc::clone(&unsubscriber);

            let jh = tokio::task::spawn(async move {
                if let Some(msg) = rx.recv().await {
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

    pub fn switch_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
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

    pub fn merge_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
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

    pub fn concat_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
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

    pub fn exhaust_map<R: 'static, F>(mut self, project: F) -> Observable<R>
    where
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
                        tokio::task::spawn(async move {

                            // Mark this inner subscription as active so other following
                            // subscriptions are rejected until this one completes.
                            *as_cloned.lock().unwrap() = true;
                            inner_observable.subscribe(inner_subscriber);
                        });
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

    fn subscribe(&mut self, v: Subscriber<Self::ObsType>) -> Subscription {
        (self.subscribe_fn)(v)
    }
}

pub struct Subscription {
    unsubscribe_logic: UnsubscribeLogic,
    subscription_future: Option<JoinHandle<()>>,
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

// fn map<T: 'static, R: 'static>(transform_fn: fn(T) -> R) -> impl FnMut(Subscriber<R>) -> Subscriber<T> {
//     move |mut oi| {
//         Subscriber::new(Box::new(move |x| {
//             oi.next(transform_fn(x));
//         }))
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc::channel;
    use tokio::task;
    use tokio::time::{sleep, Duration};

    pub fn make_emit_u32_observable(
        end: u32,
        last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
    ) -> Observable<u32> {
        let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

        Observable::new(move |mut o: Subscriber<_>| {
            let done = Arc::new(Mutex::new(false));
            let done_c = Arc::clone(&done);
            let (tx, mut rx) = channel(10);

            task::spawn(async move {
                while let Some(i) = rx.recv().await {
                    *done_c.lock().unwrap() = i;
                }
            });

            let last_emit_assert = Arc::clone(&last_emit_assert);
            let jh = task::spawn(async move {
                let mut last_emit = 0;

                for i in 0..=end {
                    if *done.lock().unwrap() == true {
                        break;
                    }
                    last_emit = i;
                    o.next(i);
                    // Important. Put an await point after each emit.
                    sleep(Duration::from_millis(1)).await;
                }
                last_emit_assert.lock().unwrap()(last_emit);
                o.complete();
            });

            Subscription::new(
                UnsubscribeLogic::Logic(Box::new(move || {
                    let tx = tx.clone();
                    task::spawn(Box::pin(async move {
                        if let Err(_) = tx.send(true).await {
                            println!("receiver dropped");
                            return;
                        }
                    }));
                })),
                Some(jh),
            )
        })
    }

    struct CheckFinished {
        last_value: i32,
        completed: bool
    }

    #[test]
    fn unchained_observable() {
        let value = 100;
        let o = Subscriber::new(
            move |v| {
                assert_eq!(
                    v, value,
                    "expected integer value {} but {} is emitted",
                    value, v
                );
            },
            Some(|observable_error| {
            }),
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            o.next(value);
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);
    }

    #[test]
    fn map_observable() {
        let last_emit_value = Arc::new(Mutex::new(
            CheckFinished { last_value: 0, completed: false }
        ));
        let last_emit_value_c1 = last_emit_value.clone();
        let last_emit_value_c2 = last_emit_value.clone();

        let value = 100;
        let o = Subscriber::new(
            move |v| {
                assert_eq!(
                    v, value,
                    "expected integer value {} but {} is emitted",
                    value, v
                );
            },
            Some(|observable_error| {
            }),
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            o.next(value);
            o.complete();
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.map(|x| {
            let y = x + 1000;
            format!("emit to str {}", y)
        });

        let o = Subscriber::new(
            move |v: String| {
                assert!(
                    v.contains("to str"),
                    "map chained observable failed, expected
                string \"{}\", got \"{}\"",
                    "emit to str",
                    v
                );
                // Make sure next is invoked.
                last_emit_value_c1.lock().unwrap().last_value = 1;
            },
            Some(|observable_error| {
            }),
            Some(move || {
                last_emit_value_c2.lock().unwrap().completed = true;
                assert!(
                    last_emit_value_c2.lock().unwrap().last_value == 1,
                    "next method not called, last emitted value should be 1, but it is {}",
                    last_emit_value_c2.lock().unwrap().last_value
                );
            }),
        );

        s.subscribe(o);
        assert!(
            last_emit_value.lock().unwrap().completed,
            "map operator did not completed observable"
        );
    }

    #[test]
    fn filter_observable() {
        let last = 10;
        let last_emit_value = Arc::new(Mutex::new(
            CheckFinished { last_value: 0, completed: false }
        ));
        let last_emit_value_c1 = last_emit_value.clone();
        let last_emit_value_c2 = last_emit_value.clone();

        let o = Subscriber::new(
            move |v| {
                assert!(v >= 0, "integer less than 0 emitted {}", v);
                assert!(v <= 10, "integer greater than 10 emitted {}", v);
            },
            Some(|observable_error| {
            }),
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            for i in 0..=last {
                o.next(i);
            }
            o.complete();
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.filter(|x| x % 2 != 0);

        let o = Subscriber::new(
            move |v| {
                assert!(
                    v % 2 != 0,
                    "filtered value expected to be odd number, got {}",
                    v
                );
                // When even numbers are filtered, last is 9.
                if v == last-1 {
                    last_emit_value_c1.lock().unwrap().last_value = v;
                }
            },
            Some(|observable_error| {
            }),
            Some(move || {
                last_emit_value_c2.lock().unwrap().completed = true;
                assert!(
                    last_emit_value_c2.lock().unwrap().last_value == last-1,
                    "last emitted value should be {}, but it is {}",
                    last,
                    last_emit_value_c2.lock().unwrap().last_value
                );
            }),
        );

        s.subscribe(o);
        assert!(
            last_emit_value.lock().unwrap().completed,
            "filter operator did not completed observable"
        );
    }

    #[test]
    fn delay_observable() {
        let last = 10;
        let last_emit_value = Arc::new(Mutex::new(
            CheckFinished { last_value: 0, completed: false }
        ));
        let last_emit_value_c1 = last_emit_value.clone();
        let last_emit_value_c2 = last_emit_value.clone();

        let o = Subscriber::new(
            move |v| {
                assert!(v >= 0, "integer less than 0 emitted {}", v);
                assert!(v <= 10, "integer greater than 10 emitted {}", v);
            },
            Some(|observable_error| {
            }),
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            for i in 0..=last {
                o.next(i);
            }
            o.complete();
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.delay(500);

        let o = Subscriber::new(
            move |v| {
                let prev = last_emit_value_c1.lock().unwrap().last_value;
                last_emit_value_c1.lock().unwrap().last_value = v;
                if v == 0 {
                    return;
                }
                assert_eq!(
                    prev, last_emit_value_c1.lock().unwrap().last_value - 1,
                    "delay operator does not emit values in order, previously emitted {}, expected {}",
                    prev,
                    prev + 1
                );
            },
            Some(|observable_error| {
            }),
            Some(move || {
                last_emit_value_c2.lock().unwrap().completed = true;
                assert!(
                    last_emit_value_c2.lock().unwrap().last_value == last,
                    "last emitted value should be {}, but it is {}",
                    last,
                    last_emit_value_c2.lock().unwrap().last_value
                );
            }),
        );

        let check_delay_cnt = Arc::new(Mutex::new(0));
        let check_delay_cnt_cloned = Arc::clone(&check_delay_cnt);
        
        // Increment counter in separate thread.
        std::thread::spawn(move || {
            for i in 0..=10 {
                *check_delay_cnt_cloned.lock().unwrap() += 1;
                std::thread::sleep(Duration::from_millis(400));
            }
        });
        s.subscribe(o);
        assert!(
            last_emit_value.lock().unwrap().completed,
            "delay operator did not completed observable"
        );

        // Check counter value set in separate thread to see if operator delayed next calls.
        assert!(
            *check_delay_cnt.lock().unwrap() > 9,
            "operator did not delayed, counter expected to be greater than {}, got {} instead",
            9,
            *check_delay_cnt.lock().unwrap()
        );
    }

    #[test]
    fn skip_observable() {
        let last = 10;
        let n = 5_i32;
        let last_emit_value = Arc::new(Mutex::new(
            CheckFinished { last_value: 0, completed: false }
        ));
        let last_emit_value_c1 = last_emit_value.clone();
        let last_emit_value_c2 = last_emit_value.clone();

        let o = Subscriber::new(
            move |v| {
                assert!(v >= 0, "integer less than 0 emitted {}", v);
                assert!(v <= 10, "integer greater than 10 emitted {}", v);
            },
            Some(|observable_error| {
            }),
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            for i in 0..=last {
                o.next(i);
            }
            o.complete();
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.skip(n.try_into().unwrap());

        let o = Subscriber::new(
            move |v| {
                assert!(
                    v > n-1,
                    "first {} values should be skipped, got {}",
                    n,
                    v
                );
                if v == last {
                    last_emit_value_c1.lock().unwrap().last_value = v;
                }
            },
            Some(|observable_error| {
            }),
            Some(move || {
                last_emit_value_c2.lock().unwrap().completed = true;
                assert!(
                    last_emit_value_c2.lock().unwrap().last_value == last,
                    "last emitted value should be {}, but it is {}",
                    last,
                    last_emit_value_c2.lock().unwrap().last_value
                );
            }),
        );

        s.subscribe(o);
        assert!(
            last_emit_value.lock().unwrap().completed,
            "skip operator did not completed observable"
        );
    }

    #[tokio::test]
    async fn take_observable() {
        let take_bound = 7_u32;
        let last_emit_value = Arc::new(Mutex::new(
            CheckFinished { last_value: 0, completed: false }
        ));
        let last_emit_value_c1 = last_emit_value.clone();
        let last_emit_value_c2 = last_emit_value.clone();

        let o = Subscriber::new(
            move |v: u32| {
                // This is executed inside task so it will not fail the test
                // if panics. We need to await the task and check for panic.
                // This test's if take(N) actually takes N number of items.
                assert!(
                    v < take_bound,
                    "exceeded take bound of {}, found {}",
                    take_bound,
                    v
                );
                if v == take_bound-1 {
                    last_emit_value_c1.lock().unwrap().last_value = v.try_into().unwrap();
                }
            },
            Some(|observable_error| {
            }),
            Some(move || {
                last_emit_value_c2.lock().unwrap().completed = true;
                assert!(
                    last_emit_value_c2.lock().unwrap().last_value == (take_bound-1).try_into().unwrap(),
                    "last emitted value should be {}, but it is {}",
                    take_bound - 1,
                    last_emit_value_c2.lock().unwrap().last_value
                );
            }),
        );

        let observable = make_emit_u32_observable(100, move |last_emit_value| {
            // Also executed inside task so it will not fail the test
            // if panics. We need to await the task and check for panic.

            // This test's if take(N) actually unsubscribes and completes
            // original observable after taking N items so it does not run in the background.
            assert!(
                last_emit_value < take_bound + 1,
                "take did not unsubscribe, take bound
is {} and last emitted value is {}",
                take_bound,
                last_emit_value
            );
        });
        let mut observable = observable.take(take_bound.try_into().unwrap());
        let s = observable.subscribe(o);

        // Await the task started in observable.
        if let Err(e) = s.join().await {
            // Check if task in observable panicked.
            if e.is_panic() {
                // If yes, resume and unwind panic to make the test fail with
                // proper error message.
                std::panic::resume_unwind(e.into_panic());
            }
        };
        assert!(
            last_emit_value.lock().unwrap().completed,
            "take operator did not completed observable"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn switch_map_observable() {
        let last_emits_count = Arc::new(Mutex::new(0_u32));
        let last_emits_count2 = Arc::clone(&last_emits_count);
        let inner_emits_cnt = Arc::new(Mutex::new(0_u32));
        let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt);
        let global_buffer = Arc::new(Mutex::new(Ok(())));
        {
            let global_buffer_clone = Arc::clone(&global_buffer);

            let o = Subscriber::new(
                |v: u32| {
                    // Noop
                },
                Some(|observable_error| {
                }),
                Some(|| {}),
            );

            let outer_o_max_count = 100;
            let inner_o_max_count = 10;

            use std::panic::catch_unwind;

            let observable = make_emit_u32_observable(outer_o_max_count, move |last_emit_value| {
                *last_emits_count2.lock().unwrap() = last_emit_value;
                // Check if original observable emitted all of the values.
                assert!(
                    last_emit_value == outer_o_max_count,
                    "outer observable did not emit all values,
last value emitted {}, expected {}",
                    last_emit_value,
                    outer_o_max_count
                );
            });

            let lock = Arc::new(Mutex::new(true));

            let mut observable = observable.switch_map(move |v| {
                let global_buffer_clone = Arc::clone(&global_buffer_clone);
                let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt2);
                let lock = Arc::clone(&lock);

                make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
                    let _guard = lock.lock().unwrap();

                    *inner_emits_cnt2.lock().unwrap() += 1;

                    // If previous inner observable panicked do not make further checks
                    // to prevent global buffer maybe being overwritten with OK(())
                    // and losing previous caught panics.
                    if global_buffer_clone.lock().unwrap().is_err() {
                        return;
                    }
                    if v < outer_o_max_count {
                        // Outer observable emitted value is 0..(outer_o_max_count - 1)
                        // and switch_map should unsubscribe every previous inner
                        // observable, consequently none of the inner observables emitted
                        // values should reach their inner_o_max_count.
                        *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                            assert!(
                                last_emit_inner_value < inner_o_max_count,
                                "switch_map did not unsubscribed inner observable properly.
Outer value is {} which is not last value emitted by outer observable. Inner observable reached
{} which is it's last value",
                                v,
                                last_emit_inner_value
                            );
                        });
                    }
                    else {
                        // Outer observable emitted value is outer_o_max_count which is last value and
                        // switch_map should not unsubscribe it consequently last inner
                        // observable subscribed should emit all it's values.
                        *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                            assert!(
                                v == outer_o_max_count,
                                "switch_map emitted more values
than it should. Expected {}, found {}",
                                outer_o_max_count,
                                v
                            );
                            assert!(
                                last_emit_inner_value == inner_o_max_count,
                                "last inner observable should have emitted all of it's values.
Expected {}, found {}",
                                inner_o_max_count,
                                last_emit_inner_value
                            );
                        });
                    }
                })
            });

            let s = observable.subscribe(o);

            // Await the task started in outer observable.
            if let Err(e) = s.join().await {
                // Check if task in outer observable panicked.
                if e.is_panic() {
                    // If yes, resume and unwind panic to make the test fail with
                    // proper error message.
                    std::panic::resume_unwind(e.into_panic());
                }
            };
            
            // Give some time to make sure all inner observables are finished.
            sleep(Duration::from_millis(3000)).await;

            assert!(
                *last_emits_count.lock().unwrap() == outer_o_max_count,
                "switch_map should have emitted {} times, but emitted {} instead",
                outer_o_max_count,
                *last_emits_count.lock().unwrap()
            );

            assert!(
                *inner_emits_cnt.lock().unwrap() != 0,
                "switch_map did not projected any of the inner observables, should project {}",
                outer_o_max_count
            );

            // Compensate for last increment
            *inner_emits_cnt.lock().unwrap() -= 1;

            assert!(
                *inner_emits_cnt.lock().unwrap() == outer_o_max_count,
                "switch_map did not projected all of the inner observables.
Projected {}, should project {}",
                *inner_emits_cnt.lock().unwrap(),
                outer_o_max_count
            );
        }
        assert!(
            Arc::strong_count(&global_buffer) == 1,
            "strong count of the global buffer is {} but should be 1",
            Arc::strong_count(&global_buffer)
        );
        let m = Arc::try_unwrap(global_buffer).unwrap();
        let m = m.into_inner().unwrap();

        if let Err(e) = m {
            std::panic::resume_unwind(e);
        };
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exhaust_map_observable() {
        let observable_occupied: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let observable_occupied2 = Arc::clone(&observable_occupied);

        let last_emits_count = Arc::new(Mutex::new(0));
        let last_emits_count2 = Arc::clone(&last_emits_count);


        let global_buffer = Arc::new(Mutex::new(Ok(())));
        {
            let global_buffer_clone = Arc::clone(&global_buffer);

            let o = Subscriber::new(
                |v: u32| {
                    // Noop
                },
                Some(|observable_error| {
                }),
                Some( move || {
                    // Set this flag to test next inner observable that should finish.
                    // XXX: But this test semaphore sometimes signals that inner observable
                    // should emit before exhaust_map actually signals it.
                    *observable_occupied2.lock().unwrap() = None;
                }),
            );

            let outer_o_max_count = 100;
            let inner_o_max_count = 10;

            use std::panic::catch_unwind;

            let observable = make_emit_u32_observable(outer_o_max_count, move |last_emit_value| {
                *last_emits_count2.lock().unwrap() = last_emit_value;
                // Check if original observable emitted all of the values.
                assert!(
                    last_emit_value == outer_o_max_count,
                    "outer observable did not emit all values,
last value emitted {}, expected {}",
                    last_emit_value,
                    outer_o_max_count
                );
            });

            let should_run = Arc::new(Mutex::new(Vec::<u32>::new()));
            let should_run_clone = Arc::clone(&should_run);
            let did_it_run = Arc::new(Mutex::new(Vec::<u32>::new()));
            let did_it_run_clone = Arc::clone(&did_it_run);

            let lock = Arc::new(Mutex::new(true));
            let project_lock = Arc::new(Mutex::new(true));

            let mut observable = observable.exhaust_map(move |v| {
                let _project_guard = project_lock.lock().unwrap();
                let global_buffer_clone = Arc::clone(&global_buffer_clone);
                let lock = Arc::clone(&lock);
                let mut should_finish = false;

                // XXX: This part is not sound. Sometimes marks value that should
                // be rejected as the one that should emit and complete and usually value after
                // that one is the one that should emit and complete and this marks it
                // for rejection.
                if observable_occupied.lock().unwrap().is_none() {
                    *observable_occupied.lock().unwrap() = Some(v);
                    
                    (*should_run_clone.lock().unwrap()).push(v);

                    should_finish = true;
                }
                // let observable_occupied3 = Arc::clone(&observable_occupied);

                let did_it_run_clone = Arc::clone(&did_it_run_clone);

                make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
                    let _guard = lock.lock().unwrap();

                    // If previous inner observable panicked do not make further checks
                    // to prevent global buffer maybe being overwritten with OK(())
                    // and losing previous caught panics.
                    if global_buffer_clone.lock().unwrap().is_err() {
                        return;
                    }

                    if should_finish {
                        // This inner observable started emitting. exhaust_map should emit all
                        // of it's values and reject any other inner observable trying
                        // to emit in the mean time.
                        *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                            assert!(
                                last_emit_inner_value == inner_o_max_count,
                                "exhaust_map should emit all values for this inner observable.
Last emitted inner value is {} but it should have reached {}",
                                last_emit_inner_value,
                                inner_o_max_count
                            );
                        });

                        (*did_it_run_clone.lock().unwrap()).push(v);

                    } else {
                        // Check that inner observables that started when previous inner observable
                        // was emitting are rejected.
                        *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                            assert!(
                                last_emit_inner_value < inner_o_max_count,
                                "exhaust_map did not unsubscribed inner observable properly.
It finished emitting inner observable that should be rejected because other inner observable
was emitting it's values. Inner observable reached it's last value {} but should have
been rejected. Outer observable is {}.",
                                last_emit_inner_value,
                                v
                            );
                        });
                    }
                })
            });
            let s = observable.subscribe(o);

            // Await the task started in outer observable.
            if let Err(e) = s.join().await {
                // Check if task in outer observable panicked.
                if e.is_panic() {
                    // If yes, resume and unwind panic to make the test fail with
                    // proper error message.
                    std::panic::resume_unwind(e.into_panic());
                }
            };
            // Make sure to give time to make sure all inner observables are finished.
            sleep(Duration::from_millis(7000)).await;

            assert!(
                *last_emits_count.lock().unwrap() == outer_o_max_count,
                "Outer observable should have emitted {} times, but emitted {} instead",
                outer_o_max_count,
                *last_emits_count.lock().unwrap()
            );

            // Check if exhaust_map emitted more values than it should.
            assert!(
                !((*should_run.lock().unwrap()).len() < (*did_it_run.lock().unwrap()).len()),
                "exhaust_map emitted more values than it should"
            );

            // Check if all inner observables that should have run actually did run.
            assert_eq!(
                *should_run.lock().unwrap(),
                *did_it_run.lock().unwrap(),
                "exhaust_map has failed to let some inner observables to run to completion"
            );
        }
        assert!(
            Arc::strong_count(&global_buffer) == 1,
            "strong count of the global buffer is {} but should be 1",
            Arc::strong_count(&global_buffer)
        );
        let m = Arc::try_unwrap(global_buffer).unwrap();
        let m = m.into_inner().unwrap();

        if let Err(e) = m {
            std::panic::resume_unwind(e);
        };
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concat_map_observable() {
        use std::panic::catch_unwind;

        let compare_outer_values = Arc::new(Mutex::new(0_u32));
        let compare_outer_values2 = Arc::clone(&compare_outer_values);
        let lock = Arc::new(Mutex::new(true));

        // Flag for determining are inner observables emitting in sequential order or not.
        let does_another_emitting = Arc::new(Mutex::new(false));
        let does_another_emitting2 = Arc::clone(&does_another_emitting);

        let global_buffer = Arc::new(Mutex::new(Ok(())));
        {
            let global_buffer_clone = Arc::clone(&global_buffer);
            let global_buffer_clone2 = Arc::clone(&global_buffer);

            let sequence_guard = Arc::new(Mutex::new(true));
            let o = Subscriber::new(
                move |v: u32| {
                    if v == 0 {
                        let _sequence_guard = sequence_guard.lock().unwrap();
                        let does_another_emitting3 = Arc::clone(&does_another_emitting);

                        // Protect global_buffer from overwriting Err() with Ok().
                        if global_buffer_clone2.lock().unwrap().is_ok() {
                            *global_buffer_clone2.lock().unwrap() = catch_unwind(move || {
                                assert!(
                                    *does_another_emitting3.lock().unwrap() == false,
                                    "concat_map started emitting next inner observable while previous one still not completed"
                                );
                            });
                            *does_another_emitting.lock().unwrap() = true;
                        }
                    }
                },
                Some(|observable_error| {
                }),
                Some(move || {
                    *does_another_emitting2.lock().unwrap() = false;
                }),
            );

            let outer_o_max_count = 200;
            let inner_o_max_count = 10;

            let observable = make_emit_u32_observable(outer_o_max_count, move |last_emit_value| {
                // Check if original observable emitted all of the values.
                assert!(
                    last_emit_value == outer_o_max_count,
                    "outer observable did not emit all values,
last value emitted {}, expected {}",
                    last_emit_value,
                    outer_o_max_count
                );
            });

            let mut observable = observable.concat_map(move |v| {
                let global_buffer_clone = Arc::clone(&global_buffer_clone);
                let compare_outer_values2 = Arc::clone(&compare_outer_values2);
                let lock = Arc::clone(&lock);

                make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
                    let _guard = lock.lock().unwrap();
                    let expected_outer_value = *compare_outer_values2.lock().unwrap();

                    // Increase by 1 to compare if next outer observable value
                    // emitted did not skip sequence.
                    *compare_outer_values2.lock().unwrap() += 1;

                    // If previous inner observable panicked do not make further checks
                    // to prevent global buffer maybe being overwritten with OK(())
                    // and losing previous caught panics.
                    if global_buffer_clone.lock().unwrap().is_err() {
                        return;
                    }

                    // Inner observables should finish in sequential order this
                    // does cover that case but not the case if some inner observables emitted before
                    // current emitting inner observable completed.
                    // Test of inner observables emitting one by one is done in root
                    // observer next() and complete() methods but that does not test
                    // are they started in sequential order e.g. 1 can start before 0.

                    *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                        // Every inner observable should finish emitting all of it's values.
                        assert!(
                            last_emit_inner_value == inner_o_max_count,
                            "concat_map should emit all values for this inner observable.
Last emitted inner value is {} but it should have reached {}",
                            last_emit_inner_value,
                            inner_o_max_count
                        );
                        assert!(
                            expected_outer_value == v,
                            "concat_map did not finished emitting values in sequential order. Next emitted
    outer observable value should have been {}, got {} instead",
                            expected_outer_value,
                            v
                        );
                    });

                })
            });
            let s = observable.subscribe(o);

            // Await the task started in outer observable.
            if let Err(e) = s.join().await {
                // Check if task in outer observable panicked.
                if e.is_panic() {
                    // If yes, resume and unwind panic to make the test fail with
                    // proper error message.
                    std::panic::resume_unwind(e.into_panic());
                }
            };
            // Make sure to give time to make sure all inner observables are finished.
            sleep(Duration::from_millis(25000)).await;

            assert!(
                *compare_outer_values.lock().unwrap() != 0,
                "concat_map did not project any of the inner observables, should project {}",
                outer_o_max_count
            );

            // Compensate for last increment
            let values_emitted_count = *compare_outer_values.lock().unwrap() - 1;

            // Check if concat_map emitted more values than it should.
            assert!(
                !(values_emitted_count > outer_o_max_count),
                "concat_map emitted more values than it should. Emitted {}, expected {}",
                values_emitted_count,
                outer_o_max_count
            );
            // Check if outer observables emitted all of the values.
            assert_eq!(
                values_emitted_count,
                outer_o_max_count,
                "concat_map has failed to project all of the inner observables.
Projected {}, should project {}",
                values_emitted_count,
                outer_o_max_count
            );
        }
        assert!(
            Arc::strong_count(&global_buffer) == 1,
            "strong count of the global buffer is {} but should be 1",
            Arc::strong_count(&global_buffer)
        );
        let m = Arc::try_unwrap(global_buffer).unwrap();
        let m = m.into_inner().unwrap();

        if let Err(e) = m {
            std::panic::resume_unwind(e);
        };
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn merge_map_observable() {
        let last_emits_count = Arc::new(Mutex::new(0_u32));
        let last_emits_count2 = Arc::clone(&last_emits_count);
        let inner_emits_cnt = Arc::new(Mutex::new(0_u32));
        let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt);
        let global_buffer = Arc::new(Mutex::new(Ok(())));
        {
            let global_buffer_clone = Arc::clone(&global_buffer);

            let o = Subscriber::new(
                |v: u32| {
                    // Noop
                },
                Some(|observable_error| {
                }),
                Some(|| {}),
            );

            let outer_o_max_count = 100;
            let inner_o_max_count = 10;

            use std::panic::catch_unwind;

            let observable = make_emit_u32_observable(outer_o_max_count, move |last_emit_value| {
                *last_emits_count2.lock().unwrap() = last_emit_value;
                // Check if original observable emitted all of the values.
                assert!(
                    last_emit_value == outer_o_max_count,
                    "outer observable did not emit all values,
last value emitted {}, expected {}",
                    last_emit_value,
                    outer_o_max_count
                );
            });

            let lock = Arc::new(Mutex::new(true));

            let mut observable = observable.merge_map(move |v| {
                let global_buffer_clone = Arc::clone(&global_buffer_clone);
                let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt2);
                let lock = Arc::clone(&lock);

                make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
                    let _guard = lock.lock().unwrap();

                    *inner_emits_cnt2.lock().unwrap() += 1;

                    // If previous inner observable panicked do not make further checks
                    // to prevent global buffer maybe being overwritten with OK(())
                    // and losing previous caught panics.
                    if global_buffer_clone.lock().unwrap().is_err() {
                        return;
                    }

                    // All projected inner observables should complete.
                    *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                        assert!(
                            last_emit_inner_value == inner_o_max_count,
                            "inner observable should have emitted all of it's values.
Expected {}, found {}",
                            inner_o_max_count,
                            last_emit_inner_value
                        );
                    });
                })
            });

            let s = observable.subscribe(o);

            // Await the task started in outer observable.
            if let Err(e) = s.join().await {
                // Check if task in outer observable panicked.
                if e.is_panic() {
                    // If yes, resume and unwind panic to make the test fail with
                    // proper error message.
                    std::panic::resume_unwind(e.into_panic());
                }
            };

            // Give some time to make sure all inner observables are finished.
            sleep(Duration::from_millis(10000)).await;

            assert!(
                *last_emits_count.lock().unwrap() == outer_o_max_count,
                "merge_map should have emitted {} times, but emitted {} instead",
                outer_o_max_count,
                *last_emits_count.lock().unwrap()
            );

            assert!(
                *inner_emits_cnt.lock().unwrap() != 0,
                "merge_map did not projected any of the inner observables, should project {}",
                outer_o_max_count
            );

            // Compensate for last increment
            *inner_emits_cnt.lock().unwrap() -= 1;

            assert!(
                *inner_emits_cnt.lock().unwrap() == outer_o_max_count,
                "merge_map did not projected all of the inner observables.
Projected {}, should project {}",
                *inner_emits_cnt.lock().unwrap(),
                outer_o_max_count
            );
        }

        assert!(
            Arc::strong_count(&global_buffer) == 1,
            "strong count of the global buffer is {} but should be 1",
            Arc::strong_count(&global_buffer)
        );
        let m = Arc::try_unwrap(global_buffer).unwrap();
        let m = m.into_inner().unwrap();

        if let Err(e) = m {
            std::panic::resume_unwind(e);
        };
    }
}

