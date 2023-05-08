#![allow(dead_code, unused_variables)]

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
}

impl<NextFnType> Subscriber<NextFnType> {
    pub fn new(
        next_fnc: impl FnMut(NextFnType) + 'static + Send + Sync,
        complete_fnc: Option<impl FnMut() + 'static + Send + Sync>,
    ) -> Self {
        let mut s = Subscriber {
            next_fn: Box::new(next_fnc),
            complete_fn: None,
        };

        if let Some(cfn) = complete_fnc {
            s.complete_fn = Some(Box::new(cfn));
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
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    let t = f(v);
                    o_shared.lock().unwrap().next(t);
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
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
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    if predicate(&v) {
                        o_shared.lock().unwrap().next(v);
                    }
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    pub fn delay(mut self, num_of_ms: u64) -> Observable<T> {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    std::thread::sleep(Duration::from_millis(num_of_ms));
                    o_shared.lock().unwrap().next(v);
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    pub fn take(mut self, n: usize) -> Observable<T> {
        let mut i = 0;

        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let (tx, mut rx) = tokio::sync::mpsc::channel(10);
            let mut signal_sent = false;

            // let tx = Arc::new(Mutex::new(tx));
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
                Some(move || {
                    o_cloned.lock().unwrap().complete();
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
            let o_cloned = Arc::clone(&o_shared);

            let project = Arc::clone(&project);

            let mut current_subscription: Option<Subscription> = None;

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned = Arc::clone(&o_shared);
                    let project = Arc::clone(&project);

                    let mut inner_observable = project.lock().unwrap()(v);
                    drop(project);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        Some(move || {
                            o_cloned.lock().unwrap().complete();
                        }),
                    );
                    let s = inner_observable.subscribe(inner_subscriber);

                    if let Some(subscription) = current_subscription.take() {
                        subscription.unsubscribe();
                    }
                    current_subscription = Some(s);
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    pub fn merge_map<R: 'static, F>(mut self, mut project: F) -> Observable<R>
    where
        F: (FnMut(T) -> Observable<R>) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned = Arc::clone(&o_shared);

                    let mut inner_observable = project(v);

                    let inner_subscriber = Subscriber::new(
                        move |k| {
                            o_shared.lock().unwrap().next(k);
                        },
                        Some(move || {
                            o_cloned.lock().unwrap().complete();
                        }),
                    );
                    inner_observable.subscribe(inner_subscriber);
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    pub fn concat_map<R: 'static, F>(mut self, mut project: F) -> Observable<R>
    where
        F: (FnMut(T) -> Observable<R>) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let pending_observables: Arc<Mutex<VecDeque<(Observable<R>, Subscriber<R>)>>> =
                Arc::new(Mutex::new(VecDeque::new()));

            let mut first_pass = true;

            let u = Subscriber::new(
                move |v| {
                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned = Arc::clone(&o_shared);
                    let po_cloned = Arc::clone(&pending_observables);

                    let mut inner_observable = project(v);
                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        Some(move || {
                            o_cloned.lock().unwrap().complete();
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
                Some(move || {
                    o_cloned.lock().unwrap().complete();
                }),
            );
            self.subscribe(u)
        })
    }

    pub fn exhaust_map<R: 'static, F>(mut self, mut project: F) -> Observable<R>
    where
        F: (FnMut(T) -> Observable<R>) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let active_subscription = Arc::new(Mutex::new(false));
            let guard = Arc::new(Mutex::new(true));

            let u = Subscriber::new(
                move |v| {
                    let as_cloned = Arc::clone(&active_subscription);
                    let as_cloned2 = Arc::clone(&active_subscription);

                    let _guard = guard.lock().unwrap();
                    let hn = *as_cloned.lock().unwrap();
                    // if hn {
                    //     println!("TRY TO SEND ??????????????????????????????");
                    //     return;
                    // }
                    // else {
                    //     println!("SENT!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                    //     *as_cloned.lock().unwrap() = true;
                    // }

                    let o_shared = Arc::clone(&o_shared);
                    let o_cloned = Arc::clone(&o_shared);

                    let mut inner_observable = project(v);

                    let inner_subscriber = Subscriber::new(
                        move |k| o_shared.lock().unwrap().next(k),
                        Some(move || {
                            o_cloned.lock().unwrap().complete();
                            *as_cloned2.lock().unwrap() = false;
                        }),
                    );
                    if !hn {
                        tokio::task::block_in_place(move || {
                            *as_cloned.lock().unwrap() = true;
                            inner_observable.subscribe(inner_subscriber);
                        });
                    }
                },
                Some(move || {
                    o_cloned.lock().unwrap().complete();
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
        let value = 100;
        let o = Subscriber::new(
            move |v| {
                assert_eq!(
                    v, value,
                    "expected integer value {} but {} is emitted",
                    value, v
                );
            },
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            o.next(value);
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.map(|x| {
            let y = x + 1000;
            format!("emit to str {}", y)
        });

        let o = Subscriber::new(
            |v: String| {
                assert!(
                    v.contains("to str"),
                    "map chained observable failed, expected
                string \"{}\", got \"{}\"",
                    "emit to str",
                    v
                )
            },
            Some(move || {}),
        );

        s.subscribe(o);
    }

    #[test]
    fn filter_observable() {
        let o = Subscriber::new(
            move |v| {
                assert!(v >= 0, "integer less than 0 emitted {}", v);
                assert!(v <= 10, "integer greater than 10 emitted {}", v);
            },
            Some(move || {}),
        );

        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            for i in 0..=10 {
                o.next(i);
            }
            Subscription::new(UnsubscribeLogic::Nil, None)
        });

        s.subscribe(o);

        let mut s = s.filter(|x| x % 2 != 0);

        let o = Subscriber::new(
            |v| {
                assert!(
                    v % 2 != 0,
                    "filtered value expected to be odd number, got {}",
                    v
                )
            },
            Some(move || {}),
        );

        s.subscribe(o);
    }

    #[tokio::test]
    async fn take_observable() {
        let take_bound = 7;
        let lev = Arc::new(Mutex::new(0));
        let lev_c = Arc::clone(&lev);
        let lev_cc = Arc::clone(&lev);

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
            },
            Some(move || {}),
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
    }

    #[tokio::test]
    async fn switch_map_observable() {
        let last_emits_count = Arc::new(Mutex::new(0));
        let last_emits_count2 = Arc::clone(&last_emits_count);
        let global_buffer = Arc::new(Mutex::new(Ok(())));
        {
            let global_buffer_clone = Arc::clone(&global_buffer);

            let o = Subscriber::new(
                move |v: u32| {
                    // Noop
                },
                Some(move || {}),
            );

            let outer_o_max_count = 10;
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
                    10
                );
            });

            let mut observable = observable.switch_map(move |v| {
                let global_buffer_clone = Arc::clone(&global_buffer_clone);
                make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
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
                            )
                        });
                    }
                    // If previous branch panicked do not enter `else` to prevent global buffer
                    // maybe being overwritten with OK(()) and ignoring previous panics.
                    else if global_buffer_clone.lock().unwrap().is_ok() {
                        // Outer observable emitted value is 10 which is last value and
                        // switch_map should not unsubscribe it consequently last inner
                        // observable subscribed should emit all it's values.
                        *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                            assert!(
                                v == outer_o_max_count,
                                "outer observable emitted more values
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
            assert!(
                *last_emits_count.lock().unwrap() == outer_o_max_count,
                "Outer observable should have emitted {} times, but emitted {} instead",
                outer_o_max_count,
                *last_emits_count.lock().unwrap()
            );

            // Give some time to make sure all inner observables are finished.
            sleep(Duration::from_millis(1000)).await;
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
