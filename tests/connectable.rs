mod generate_observable;

use std::sync::{Arc, Mutex};

use generate_observable::generate_u32_observable;

use rxr::{subscribe::Subscriber, ObservableExt, Subscribeable};

#[test]
fn connectable_observable() {
    let emitted = Arc::new(Mutex::new(Vec::with_capacity(27)));
    let emitted_cl1 = Arc::clone(&emitted);
    let emitted_cl2 = Arc::clone(&emitted);
    let emitted_cl3 = Arc::clone(&emitted);

    let observer1 = Subscriber::on_next(move |v| {
        emitted_cl1.lock().unwrap().push(v);
    });
    let observer2 = Subscriber::on_next(move |v| {
        emitted_cl2.lock().unwrap().push(v);
    });
    let observer3 = Subscriber::on_next(move |v| {
        emitted_cl3.lock().unwrap().push(v);
    });

    let observable = generate_u32_observable(8, |_| {});

    let mut connectable = observable.connectable();

    connectable.subscribe(observer1);
    connectable.subscribe(observer2);
    connectable.subscribe(observer3);

    let emitted_guard = emitted.lock().unwrap();
    let emitted_ref: &[u32] = emitted_guard.as_ref();

    assert_eq!(
        emitted_ref.len(),
        0,
        "connectable observable emitted values before calling `connect()`"
    );

    drop(emitted_guard);

    let s = connectable.connect();
    let _ = s.join();

    let emitted_guard = emitted.lock().unwrap();
    let emitted_ref: &[u32] = emitted_guard.as_ref();

    assert_ne!(
        emitted_ref.len(),
        0,
        "connectable observable failed to emit values after calling `connect()`"
    );

    assert_eq!(
        emitted_ref.len(),
        27,
        "connectable observable emitted wrong number of values"
    );
}
