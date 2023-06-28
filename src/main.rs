use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rxr::*;

use tokio::sync::mpsc::channel;
use tokio::time::{sleep, Duration};
use tokio::task;

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
    some_ref: Rc<SomeStruct>,
}

impl ObjectAnother {
    fn print(&self) {
        println!("In another object ----- {} {}", self.v, self.some_ref.0);
    }
}

struct SomeStruct(usize);

#[tokio::main]
async fn main() {
    let o = Subscriber::new(|v| {
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
        let (tx, mut rx) = channel(10);

        task::spawn(async move {
            while let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }

        });

        let jh = task::spawn(async move {
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
                sleep(Duration::from_millis(1)).await;
            }
            o.complete();
        });

       // Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
       //     let tx = tx.clone();
       //     task::spawn(Box::pin(async move {
       //         if let Err(_) = tx.send(true).await {
       //             println!("receiver dropped");
       //             return;
       //         }
       //     }));
       //})), Some(jh))
        Subscription::new(UnsubscribeLogic::Future(Box::pin(async move {
            if (tx.send(true).await).is_err() {
                println!("receiver dropped");
            }
        })), Some(jh))
       // let obs = Observable::new(move |_s| {
       //    let tx = tx.clone();
       //    Subscription::new(UnsubscribeLogic::Logic(Box::new(move || {
       //        let tx = tx.clone();
       //        task::spawn(Box::pin(async move {
       //            println!("~~~~~~~~~~~~~~~~~ UNSUBS NOW ~~~~~~~~~~~~~~~~~~");
       //            if let Err(_) = tx.send(true).await {
       //                println!("receiver dropped");
       //                return;
       //            }
       //        }));
       //    })), None)
       //}).subscribe(Subscriber::new(|_: usize| {}, None::<fn()>));
       // Subscription::new(UnsubscribeLogic::Wrapped(Box::new(obs)), None)
    });

    // let mut s = s.filter(|x| { x % 2 != 0 } );
    // -- s.subscribe(o);
 
   // -- let mut s = s.map(move |x| {
   // --     let y = x + 1000;
   // --     format!("to str {}", y)
   // --     //  x.print();
   // --     //  let val = SomeStruct(19);
   // --     //  ObjectAnother { v: 1000, some_ref: Rc::new(val) }
   // -- });
    
   // let mut s = s.delay(190);
   let mut s = s.skip(8);

  // --  let o = Subscriber::new(|v| {
  // --       // task::spawn(async move {
  // --       // sleep(Duration::from_secs(3)).await;
  // --      println!("----- {:?}", v);
  // --       // });
  // --  }, Some(|| { println!("Completed ------ .................") })
  // --  );
   
   // let handle = s.subscribe(Subscriber::new(|v| { println!("##### {:?}", v) }
   //     , None::<fn()>
   //     ));

   // -- let mut s = s.exhaust_map(move |sval| {
   // --     println!("in projected {}", sval);
   // --    // -- bar(sval).map(move |n| {
   // --    // --     format!("In inner observable {}", n)
   // --    // -- }).delay(10)
   // --     bar(sval.to_string(), move |lev| {
   // --             println!("AFTER CAPTURE {}", lev);
   // --     })
   // -- });

    // let mut s = s.delay(100);
    let mut s = s.map(|mut a| {
        // a.print(); a.v += 15;
        // a
        format!("{} TEST", a)
    });

    // let mut s = s.take(20); // Bad place to call take()
    let handle = s.subscribe(o);
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

    sleep(Duration::from_secs(19)).await;
    // let  Some(y) = handle.lock().unwrap().take()
    // else {
    //    return; 
    // };
    
    if let Err(err) = handle.join().await {
        println!("{}", err);
    }
   // sleep(Duration::from_secs(3)).await;
    
    // handle.observable_handle.abort();

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
            for i in 0..=10 {
                let v = v.clone();
                if *done.lock().unwrap() {
                    break;
                }
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
        })), Some(jh))
    })
}
