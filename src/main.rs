#![allow(dead_code, unused_variables)]

use std::error::Error;
use std::fmt::Display;
use std::sync::{Arc, Mutex};

use rxr::*;

use rxr::subjects::{BehaviorSubject, ReplaySubject, BufSize, AsyncSubject};
use rxr::subscribe::{Subscriber, Subscription, UnsubscribeLogic, SubscriptionHandle, Unsubscribeable};
use tokio::sync::mpsc::channel;
use tokio::time::{sleep, Duration};
use tokio::task;

use rxr::observer::Observer;

struct ObjectTestOriginal {
    pub v: i128,
}

impl ObjectTestOriginal {
    fn print(&self) {
        println!("----- {}", self.v);
    }

}

struct ObjectAnother {
    pub v: i128,
    some_ref: Arc<SomeStruct>,
}

impl ObjectAnother {
    fn print(&self) {
        println!("In another object ----- {} {}", self.v, self.some_ref.0);
    }
}

struct SomeStruct(usize);

#[tokio::main()]
async fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");

    let o = Subscriber::new(|v: i32| {
        println!("----- {}", v);
        // v.print();
        },
        Some(|observable_error| {
            println!("{}", observable_error);
        }),
        Some(|| { println!("Completed .................") })
    );
    
    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

       std::thread::spawn(move || {
           while let Ok(i) = rx.recv() {
               *done_c.lock().unwrap() = i;
           }

       });

        let jh = std::thread::spawn(move || {
            for i in 0..=100 {
                if *done.lock().unwrap() {
                    break;
                }
                println!("In original Subcriber {}", i);
                // let d = ObjectTestOriginal { v: i };

                o.next(i);
                // let ds = format!("Error #{}", i);
                // o.error(ObservableError::Info(ds));
                // Important. Put an await point after each emit.
                std::thread::sleep(Duration::from_millis(1));
                //std::thread::yield_now();
            }
            o.complete();
        });

        Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
               if let Err(_) = tx.send(true) {
                   println!("receiver dropped");
                   return;
               }
           });
       })), SubscriptionHandle::JoinThread(jh))
       // Subscription::new(UnsubscribeLogic::Future(Box::pin(async move {
       //     if (tx.send(true).await).is_err() {
       //         println!("receiver dropped");
       //     }
       // })), SubscriptionHandle::JoinThread(jh))
       // let obs = Observable::new(move |_s| {
       //      let tx = tx.clone();

       //      Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
       //          let tx = tx.clone();
       //          let _ = std::thread::spawn(move || {
       //              println!("~~~~~~~~~~~~~~~~~ UNSUBS NOW ~~~~~~~~~~~~~~~~~~");
       //              if let Err(_) = tx.blocking_send(true) {
       //                  println!("receiver dropped");
       //                  return;
       //              }
       //          }).join();
       //      })), SubscriptionHandle::Nil)
       //  }).subscribe(Subscriber::new(|_: usize| {}, None::<fn(_)>, None::<fn()>));
       //  Subscription::new(UnsubscribeLogic::Wrapped(Box::new(obs)), SubscriptionHandle::Nil)
    });

    let mut s = s.take(7);
    // let mut s = s.filter(|x| { x % 2 != 0 } );
    let us = s.subscribe(o);

    // let handle = match us.get_handle() {
    //     SubscriptionHandle::JoinThread(h) => {
    //         let h = h.join();
    //         h.map_err(|e| SubscriptionError::JoinThreadError(e))
    //     },
    //     _ => {
    //         Err(SubscriptionError::JoinThreadError(Box::new(())))
    //     }
    // };
 
    match us.join_thread() {
        Ok(_) => (),
        Err(e) => (),
    }

    let mut s = s.map(move |x| {
        let y = x + 1000;
        format!("to str {}", y)
       //  x.print();
       //  let val = SomeStruct(19);
       //  ObjectAnother { v: 1000, some_ref: Rc::new(val) }
    });
    
    let mut s = s.delay(190);
    // let mut s = s.take(15);

    let o = Subscriber::new(|v: u64| {
        // task::spawn(async move {
        // sleep(Duration::from_secs(3)).await;
        println!("----- {:?}", v);
        // });
        },
        None::<fn(_)>,
        Some(|| { println!("Completed ------ .................") })
    );
   
    // ------- task::spawn(async move {
    // -------     s.subscribe(o);
    // ------- });

   // let handle = s.subscribe(Subscriber::new(|v| { println!("##### {:?}", v) }
   //     , None::<fn()>
   //     ));

   let mut s = s.switch_map(move |sval| {
       println!("in projected {}", sval);
      // -- bar(sval).map(move |n| {
      // --     format!("In inner observable {}", n)
      // -- }).delay(10)
       baz(sval.to_string(), move |lev| {
               println!("AFTER CAPTURE {}", lev);
       }).delay(10)
   });

   // let mut s = s.delay(100);
   // -------  let mut s = s.map(|mut a| {
   // -------      // a.print(); a.v += 15;
   // -------      // a
   // -------      format!("{} TEST", a)
   // -------  });

    // let mut s = s.take(20); // Bad place to call take()
    // ----- let handle = s.subscribe(o);
    // let _ = handle.join().await;

    // let mut handle = Arc::new(Mutex::new(Some(handle)));
    // let handle_cloned = Arc::clone(&handle);
    // handle.unsubscribe();
    // sleep(Duration::from_secs(5)).await;
    for i in 0..=10 {
        // if i == 3 { handle.take().map(|h| { h.unsubscribe(); }); }
        println!("continue main thread {}", i);
        // sleep(Duration::from_secs(2)).await;
    }

   // task::spawn(async move {
   //     sleep(Duration::from_nanos(9999999)).await;
   //     handle_cloned.lock().unwrap().take().map(|y| {
   //    //     y.unsubscribe();
   //     });
   // });

    // -------- sleep(Duration::from_secs(5)).await;
    // let  Some(y) = handle.lock().unwrap().take()
    // else {
    //    return; 
    // };
    
   // -------- if let Err(err) = handle.join().await {
   // --------     println!("{}", err);
   // -------- }
   // sleep(Duration::from_secs(3)).await;
    
    // handle.observable_handle.abort();

    #[derive(Debug)]
    struct MyErr(i32);

    impl Display for MyErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
               write!(f, "(Test emit error {:?})", self)
           }   
    }

    impl Error for MyErr {}

    let (mut stx, mut srx) = AsyncSubject::new();

    stx.next("100009".to_string());

    let _ = sleep(Duration::from_millis(1)).await;
    srx.subscribe(
        Subscriber::new(|x| { println!("UNCHAINED: x is {}", x); },
        Some(|_| { println!("error 1 called"); }),
        Some(|| { println!("completed 1 called"); }))
    );

    srx.clone().map(
        |x| { format!("'{} stringified'", x) })
        .subscribe(Subscriber::new(|x| { println!("mapped x is {}", x); },
        Some(|_| { println!("error 2 called"); }),
        Some(|| { println!("completed 2 called"); }))
    );

    let subscr = srx.clone()
        // .filter(|x| *x < 1000)
        .map(|x| { format!("'{} stringified'", x) })
        // .merge_map(|v| { baz(v, |_| {}) })
        // .concat_map(|l| bar(l, |_| {}))
        .filter(|_| true)
        .subscribe(Subscriber::new(|x| { println!("mapped still x is {}", x); },
        Some(|_| { println!("error 3 called"); }),
        Some(|| { println!("completed 3 called"); }))
    );

    // let srx = srx.fuse();

    // stx.next("1".to_string());

    let mut test_subject_as_subscriber = baz("sas".to_string(), |_| {});
    // stx.next(19.to_string());
    // stx.next(190.to_string());
    
    let mut stx_thread = stx.clone();
    let srx_thread = srx.clone();
    std::thread::spawn(move || {
       // stx_thread.next("-> 988".to_string());
        // let _ = std::thread::sleep(Duration::from_millis(5));
        stx_thread.complete();
    });

    // let _ = sleep(Duration::from_millis(5)).await;

    // subscr.unsubscribe();
    stx.complete();
    //stx.error(Arc::new(MyErr(8)));
    stx.next(290.to_string());

    srx.subscribe(
        Subscriber::new(|x| { println!("SUBSCRIBE AFTER COMPLETE: x is {}", x); },
        Some(|e| { println!("error after complete 1 called {}", e); }),
        Some(|| { println!("completed after complete 1 called"); }))
    );

    let mut srx2 = srx.clone();
    // srx.unsubscribe();
    srx2.subscribe(
        Subscriber::new(|x| { println!("SUBSCRIBE AFTER COMPLETE: x is {}", x); },
        Some(|e| { println!("error after complete 2 called {}", e); }),
        Some(|| { println!("completed after complete 2 called"); }))
    );
    stx.next(88888.to_string());
    // let mut stxcl = stx.clone();
    // let mut stxclt = stx.clone();
    // let mut srxcl = srx.clone();

    // let tstsubs = Subscriber::new(|x| { println!("in second thread"); },
    //         Some(|_| {}),
    //         None::<fn()>);

    // let handle = std::thread::spawn(move || {
    //     let _ = std::thread::sleep(Duration::from_secs(1));
    //     srxcl.subscribe(tstsubs);
    //     stxclt.next("80".to_string());
    //     // let _ = std::thread::sleep(Duration::from_millis(20));
    //     // stxclt.error(Rc::new(MyErr));
    // });

    // --------- let mut tst0 = test_subject_as_subscriber.take(3);
    // --------- let sbs = tst0.subscribe(stx.clone().into());

    //stx.complete();
    // tst0.subscribe(Subscriber::new(
    //     |v| {
    //         println!("Other Subscriber {}", v)
    //     },
    //     Some(|_| { println!("Other Subscriber Error") }),
    //     Some(|| { println!("Other Subscriber Complete") })).into()
    // );

    // stxcl.next("398".to_string());
    // let _ = std::thread::sleep(Duration::from_secs(3));
    // stxcl.complete();

    // println!("{}", srx.len());
    // let _ = handle.join();

    // println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
    // let (mut stx, mut srx) = Subject::<i32>::new();

    // srx.subscribe(Subscriber::new(|v| { println!("S*. - {}", v); }, None::<fn(_)>, None::<fn()>));
    // 
    // stx.next(1);

    // srx.clone().map(|v| { format!("#{}", v) }).subscribe(Subscriber::new(|v| { println!("S**. - {}", v); }, None::<fn(_)>, None::<fn()>));

    // stx.next(2);

    // srx.clone().map(|v| { (format!("#{}", v), 10) }).subscribe(Subscriber::new(|v| { println!("S***. - {:?}", v); }, None::<fn(_)>, None::<fn()>));

    // stx.next(3);

    let _ = sleep(Duration::from_secs(5)).await;
}

fn bar(v: String,
        last_emit_assert: impl FnMut(String) + Send + Sync + 'static,
) -> Observable<String> {

    // let v_shared = Arc::new(Mutex::new(v));
    // let v_clone = Arc::clone(&v_shared);

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

        let mut v = v.clone();
        // let v_clone = Arc::clone(&v_clone);
        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = task::spawn(async move {
            
            let mut last_emit = 0;
            for i in 0..=100 {
                let v = v.clone();
                if *done.lock().unwrap() {
                    break;
                }
               // if i == 4 {
               //     o.error(ObservableError::NoInfo);
               // }
                last_emit = i;
                o.next(format!("iv = {} ov = {}", i, v));
                sleep(Duration::from_millis(1)).await;
            }
            v.push_str("  --");
            last_emit_assert.lock().unwrap()(v);
            o.complete();
        });

       Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
           // let tx = tx.clone();
           task::spawn(Box::pin(async move {
               if (tx.send(true).await).is_err() {
                   println!("receiver dropped");
               }
           }));
        })), SubscriptionHandle::JoinTask(jh))
    })
}

fn baz(v: String,
        last_emit_assert: impl FnMut(String) + Send + Sync + 'static,
) -> Observable<String> {

    // let v_shared = Arc::new(Mutex::new(v));
    // let v_clone = Arc::clone(&v_shared);

    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));
    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        std::thread::spawn(move || {
            while let Some(i) = rx.blocking_recv() {
                *done_c.lock().unwrap() = i;
            }

        });

        let mut v = v.clone();
        // let v_clone = Arc::clone(&v_clone);
        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = std::thread::spawn(move || {
            
            let mut last_emit = 0;
            for i in 0..=10 {
                let v = v.clone();
                if *done.lock().unwrap() {
                    break;
                }
               // if i == 4 {
               //     o.error(ObservableError::NoInfo);
               // }
                last_emit = i;
                o.next(format!("iv = {} ov = {}", i, v));
                std::thread::sleep(Duration::from_millis(1));
            }
            v.push_str("  --");
            last_emit_assert.lock().unwrap()(v);
            o.complete();
        });

        Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
        let tx = tx.clone();
          std::thread::spawn(move || {
              if (tx.blocking_send(true)).is_err() {
                  println!("receiver dropped");
              }
          });
        })), SubscriptionHandle::JoinThread(jh))
    })
}

fn baz_sync(v: String,
        last_emit_assert: impl FnMut(String) + Send + Sync + 'static,
) -> Observable<String> {

    // let v_shared = Arc::new(Mutex::new(v));
    // let v_clone = Arc::clone(&v_shared);

    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));
    Observable::new(move |mut o: Subscriber<_>| {

        let mut v = v.clone();
            
        let mut last_emit = 0;
        for i in 0..=10 {
            let v = v.clone();
            last_emit = i;
            o.next(format!("iv = {} ov = {}", i, v));
        }
        v.push_str("  --");
        last_emit_assert.lock().unwrap()(v);
        o.complete();

       Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    })
}
