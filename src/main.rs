use std::sync::{Arc, Mutex};

use rxr::*;

use tokio::sync::mpsc::channel;
use tokio::time::{sleep, Duration};
use tokio::task;

#[tokio::main]
async fn main() {
   let o = Subscriber::new(|v| {
       println!("----- {}", v);
   }, Some(|| { println!("Completed .................") })
);
    
    let s = Observable::new(|mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        task::spawn(async move {
            while let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }

        });

        task::spawn(async move {
            for i in 0..=10 {
                if *done.lock().unwrap() == true {
                    break;
                }
                o.next(i);
            }
            o.complete();
        });

       // UnsubscribeLogic::Logic(Box::new(move || {
       //     let tx = tx.clone();
       //     task::spawn(Box::pin(async move {
       //         if let Err(_) = tx.send(true).await {
       //             println!("receiver dropped");
       //             return;
       //         }
       //     }));
       // }))
       // UnsubscribeLogic::Future(Box::pin(async move {
       //     if let Err(_) = tx.send(true).await {
       //         println!("receiver dropped");
       //         return;
       //     }
       // }))
        UnsubscribeLogic::Wrapped(Box::new(Observable::new(move |_s| {
            let tx = tx.clone();
            UnsubscribeLogic::Logic(Box::new(move || {
                let tx = tx.clone();
                task::spawn(Box::pin(async move {
                    if let Err(_) = tx.send(true).await {
                        println!("receiver dropped");
                        return;
                    }
                }));
            }))
        }).subscribe(Subscriber::new(|_: usize| {}, None::<fn()>))))
    });

    let mut s = s.filter(|x| { x % 2 != 0 } );
    s.subscribe(o);
 
    let mut s = s.map(|x| {
        let y = x + 1000;
        format!("to str {}", y)
    });
    
    // let s = s.take(2);
     let mut s = s.delay(2);

    let o = Subscriber::new(|v| {
         // task::spawn(async move {
         // sleep(Duration::from_secs(3)).await;
        println!("----- {:?}", v);
         // });
    }, Some(|| { println!("Completed ------ .................") })
    );
    
   // let handle = s.subscribe(Subscriber::new(|v| { println!("##### {:?}", v) }
   //     , None::<fn()>
   //     ));

    let s = s.exhaust_map(move |s| {
        println!("in projected {}", s);
        bar(s).map(move |n| {
            format!("In inner observable {}", n)
        })
    });

    // let mut s = s.delay(3);
    let mut s = s.map(|a| format!("{} TEST", a) );

    let handle = s.subscribe(o);
    // let _ = handle.join().await;

//    handle.unsubscribe();
    // sleep(Duration::from_secs(5)).await;
    for i in 0..=10 {
        // if i == 3 { handle.unsubscribe(); }
        println!("continue main thread {}",  i);
        sleep(Duration::from_secs(3)).await;
    }
    
   // sleep(Duration::from_secs(3)).await;
    
    // handle.observable_handle.abort();

}

fn foo<T: Send + Sync + 'static>(v: T) -> Observable<i32> {

    let v_shared = Arc::new(Mutex::new(v));
    let v_clone = Arc::clone(&v_shared);

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        task::spawn(async move {
            while let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }

        });

        let v_clone = Arc::clone(&v_clone);
        task::spawn(async move {
            
            let f = v_clone;
            for i in 0..=10 {
                if *done.lock().unwrap() == true {
                    break;
                }
                o.next(i);
            }
            o.complete();
        });

       UnsubscribeLogic::Logic(Box::new(move || {
           let tx = tx.clone();
           task::spawn(Box::pin(async move {
               if let Err(_) = tx.send(true).await {
                   println!("receiver dropped");
                   return;
               }
           }));
        }))
    })
}

fn bar(v: String) -> Observable<String> {

    // let v_shared = Arc::new(Mutex::new(v));
    // let v_clone = Arc::clone(&v_shared);

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, mut rx) = channel(10);

        task::spawn(async move {
            while let Some(i) = rx.recv().await {
                *done_c.lock().unwrap() = i;
            }

        });

        let v = v.clone();
        // let v_clone = Arc::clone(&v_clone);
        task::spawn(async move {
            
            for i in 0..=200 {
                let v = v.clone();
                if *done.lock().unwrap() == true {
                    break;
                }
                o.next(format!("{} {}", i, v));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            o.complete();
        });

       UnsubscribeLogic::Logic(Box::new(move || {
           let tx = tx.clone();
           task::spawn(Box::pin(async move {
               if let Err(_) = tx.send(true).await {
                   println!("receiver dropped");
                   return;
               }
           }));
        }))
    })
}
