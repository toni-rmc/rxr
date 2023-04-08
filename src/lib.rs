#![allow(dead_code, unused_variables)]

use std::{time::Duration, sync::{Arc, Mutex, mpsc::{channel, sync_channel}}, future::Future, pin::Pin, collections::VecDeque, os::windows::thread};

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
            },
            UnsubscribeLogic::Wrapped(subscription) => {
                subscription.unsubscribe();
                self = Self::Nil;
            },
            UnsubscribeLogic::Future(future) => {
                tokio::task::spawn(async move {
                    future.await;
                });
                self = Self::Nil;
            },
        }
        self
    }
}

pub struct Subscriber<NextFnType> {
    next_fn: Box<dyn FnMut(NextFnType) + Send + Sync>,
    complete_fn: Option<Box<dyn FnMut() + Send + Sync>>,
}

impl<NextFnType> Subscriber<NextFnType> {   
    pub fn new(next_fnc: impl FnMut(NextFnType) + 'static + Send + Sync,
               complete_fnc: Option<impl FnMut() + 'static + Send + Sync>
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
 
impl<'a, N> Observer for Subscriber<N> {
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
    subscribe_fn: Box<dyn FnMut(Subscriber<T>) -> UnsubscribeLogic + Send + Sync>,
}

impl<T: 'static> Observable<T> {
    pub fn new(sf: impl FnMut(Subscriber<T>) -> UnsubscribeLogic + Send + Sync + 'static) -> Self {
        Observable {
            subscribe_fn: Box::new(sf)
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

            let u = Subscriber::new(move |v| {
                let t = f(v);
                o_shared.lock().unwrap().next(t);
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
        })
    }

    pub fn filter<P>(mut self, predicate: P) -> Observable<T>
    where
        P: (FnOnce(&T) -> bool) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(move |v| {
                if predicate(&v) {
                    o_shared.lock().unwrap().next(v);
                }
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
        })
    }

    pub fn delay(mut self, num_of_sec: u64) -> Observable<T> {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(move |v| {
                std::thread::sleep(Duration::from_secs(num_of_sec));
                o_shared.lock().unwrap().next(v);
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
        })
    }

    pub fn take(mut self, n: usize) -> Observable<T> {
        let mut i = 0;

        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let u = Subscriber::new(move |v| {
                if i < n {
                    i += 1;
                    o_shared.lock().unwrap().next(v);
                }
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
        })
    }

    pub fn switch_map<R: 'static, F>(mut self, mut project: F) -> Observable<R>
    where
        F: (FnMut(T) -> Observable<R>) + Copy + Sync + Send + 'static,
    {
        Observable::new(move |o| {
            let o_shared = Arc::new(Mutex::new(o));
            let o_cloned = Arc::clone(&o_shared);

            let mut current_subscription: Option<Subscription> = None;

            let u = Subscriber::new(move |v| {

                if let Some(subscription) = current_subscription.take() {
                    subscription.unsubscribe();
                }
                let o_shared = Arc::clone(&o_shared);
                let o_cloned = Arc::clone(&o_shared);

                let mut inner_observable = project(v);
                
                let inner_subscriber = Subscriber::new(move |k| {
                        o_shared.lock().unwrap().next(k)
                    },
                    Some(move || { o_cloned.lock().unwrap().complete(); })
                );
                current_subscription = Some(inner_observable.subscribe(inner_subscriber));
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
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

            let u = Subscriber::new(move |v| {

                let o_shared = Arc::clone(&o_shared);
                let o_cloned = Arc::clone(&o_shared);
                let po_cloned = Arc::clone(&pending_observables);

                let mut inner_observable = project(v);
                let inner_subscriber = Subscriber::new(move |k| {
                    o_shared.lock().unwrap().next(k)
                },
                    Some(move || {
                        o_cloned.lock().unwrap().complete();
                        if let Some((mut io, is)) = po_cloned.lock().unwrap().pop_front() {
                            io.subscribe(is);
                        }
                    })
                );

                if first_pass {
                    inner_observable.subscribe(inner_subscriber);
                    first_pass = false;
                    return;
                }
                pending_observables.lock().unwrap().push_back((inner_observable, inner_subscriber));
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
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

            let u = Subscriber::new(move |v| {

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
                
                let inner_subscriber = Subscriber::new(move |k| {
                        o_shared.lock().unwrap().next(k)
                    },
                    Some(move || {
                        o_cloned.lock().unwrap().complete();
                        *as_cloned2.lock().unwrap() = false;
                    })
                );
                if !hn {
                    tokio::task::block_in_place(move || {
                    *as_cloned.lock().unwrap() = true;
                    inner_observable.subscribe(inner_subscriber);
                    });
                }
            }, Some(move || { o_cloned.lock().unwrap().complete(); })
            );
            (self.subscribe_fn)(u)
        })
    }
}

impl<T: 'static> Subscribeable for Observable<T> {
    type ObsType = T;

    fn subscribe(&mut self, v: Subscriber<Self::ObsType>) -> Subscription {

        let unsubscribe_logic = (self.subscribe_fn)(v);
        
        Subscription {
            unsubscribe_logic
        }
    }
}

pub struct Subscription {
    unsubscribe_logic: UnsubscribeLogic,
}

impl Subscription {
    
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

    #[test]
    fn unchained_observable_emit() {
        let value = 100;
        let o = Subscriber::new(move |v| {
            assert_eq!(v, value, "expected integer value {} but {} is emitted", value, v);
        }, Some(move || { })
        );
        
        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            o.next(value);
            UnsubscribeLogic::Nil
        });

        s.subscribe(o);
    }

    #[test]
    fn map_chained_observable_emit() {
        let value = 100;
        let o = Subscriber::new(move |v| {
            assert_eq!(v, value, "expected integer value {} but {} is emitted", value, v);
        }, Some(move || { })
        );
        
        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            o.next(value);
            UnsubscribeLogic::Nil
        });

        s.subscribe(o);

        let mut s = s.map(|x| {
            let y = x + 1000;
            format!("emit to str {}", y)
        });
        
        let o = Subscriber::new(|v: String| {
            assert!(v.contains("to str"), "map chained observable failed, expected
                string \"{}\", got \"{}\"", "emit to str", v)
        }, Some(move || { })
        );
        
        s.subscribe(o);
    }

    #[test]
    fn filter_chained_observable_emit() {
        let o = Subscriber::new(move |v| {
            assert!(v >= 0,  "integer less than 0 emitted {}", v);
            assert!(v <= 10,  "integer greater than 10 emitted {}", v);
        }, Some(move || { })
        );
        
        let mut s = Observable::new(move |mut o: Subscriber<_>| {
            for i in 0..=10 {
                o.next(i);
            }
            UnsubscribeLogic::Nil
        });

        s.subscribe(o);

        let mut s = s.filter(|x| { x % 2 != 0 });
        
        let o = Subscriber::new(|v| {
            assert!(v % 2 != 0, "filtered value expected to be odd number, got {}", v)
        }, Some(move || { })
        );
        
        s.subscribe(o);
    }
}
