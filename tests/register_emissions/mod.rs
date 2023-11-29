use std::sync::{Arc, Mutex};

use rxr::subscribe::Subscriber;

pub fn register_emissions_subscriber() -> (
    Vec<impl FnOnce() -> Subscriber<i32>>,
    Arc<Mutex<Vec<i32>>>,
    Arc<Mutex<Vec<i32>>>,
    Arc<Mutex<Vec<i32>>>,
) {
    let nexts: Vec<i32> = Vec::with_capacity(5);
    let nexts = Arc::new(Mutex::new(nexts));
    let nexts_c = Arc::clone(&nexts);

    let completes: Vec<i32> = Vec::with_capacity(5);
    let completes = Arc::new(Mutex::new(completes));
    let completes_c = Arc::clone(&completes);

    let errors: Vec<i32> = Vec::with_capacity(5);
    let errors = Arc::new(Mutex::new(errors));
    let errors_c = Arc::clone(&errors);

    let make_subscriber = vec![
        move || {
            Subscriber::new(
                move |n| {
                    // Track next() calls.
                    nexts_c.lock().unwrap().push(n);
                },
                move |_| {
                    // Track error() calls.
                    errors_c.lock().unwrap().push(1);
                },
                move || {
                    // Track complete() calls.
                    completes_c.lock().unwrap().push(1);
                },
            )
        };
        10
    ];
    (make_subscriber, nexts, completes, errors)
}
