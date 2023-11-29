mod generate_observable;

use generate_observable::generate_u32_observable;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, Observer,
};

use rxr::{ObservableExt, Subscribeable};

struct CheckFinished {
    last_value: i32,
    completed: bool,
}

#[test]
fn map_observable() {
    let last_emit_value = Arc::new(Mutex::new(CheckFinished {
        last_value: 0,
        completed: false,
    }));
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
        |_observable_error| {},
        move || {},
    );

    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        o.next(value);
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
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
        |_observable_error| {},
        move || {
            last_emit_value_c2.lock().unwrap().completed = true;
            assert!(
                last_emit_value_c2.lock().unwrap().last_value == 1,
                "next method not called, last emitted value should be 1, but it is {}",
                last_emit_value_c2.lock().unwrap().last_value
            );
        },
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
    let last_emit_value = Arc::new(Mutex::new(CheckFinished {
        last_value: 0,
        completed: false,
    }));
    let last_emit_value_c1 = last_emit_value.clone();
    let last_emit_value_c2 = last_emit_value.clone();

    let o = Subscriber::new(
        move |v| {
            assert!(v >= 0, "integer less than 0 emitted {}", v);
            assert!(v <= 10, "integer greater than 10 emitted {}", v);
        },
        |_observable_error| {},
        move || {},
    );

    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        for i in 0..=last {
            o.next(i);
        }
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
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
            if v == last - 1 {
                last_emit_value_c1.lock().unwrap().last_value = v;
            }
        },
        |_observable_error| {},
        move || {
            last_emit_value_c2.lock().unwrap().completed = true;
            assert!(
                last_emit_value_c2.lock().unwrap().last_value == last - 1,
                "last emitted value should be {}, but it is {}",
                last,
                last_emit_value_c2.lock().unwrap().last_value
            );
        },
    );

    s.subscribe(o);
    assert!(
        last_emit_value.lock().unwrap().completed,
        "filter operator did not completed observable"
    );

    assert!(
        last_emit_value.lock().unwrap().last_value == last - 1,
        "filter operator did not emit"
    );
}

#[test]
fn delay_observable() {
    let last = 10;
    let last_emit_value = Arc::new(Mutex::new(CheckFinished {
        last_value: 0,
        completed: false,
    }));
    let last_emit_value_c1 = last_emit_value.clone();
    let last_emit_value_c2 = last_emit_value.clone();

    let o = Subscriber::new(
        move |v| {
            assert!(v >= 0, "integer less than 0 emitted {}", v);
            assert!(v <= 10, "integer greater than 10 emitted {}", v);
        },
        |_observable_error| {},
        move || {},
    );

    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        for i in 0..=last {
            o.next(i);
        }
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
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
                prev,
                last_emit_value_c1.lock().unwrap().last_value - 1,
                "delay operator does not emit values in order, previously emitted {}, expected {}",
                prev,
                prev + 1
            );
        },
        |_observable_error| {},
        move || {
            last_emit_value_c2.lock().unwrap().completed = true;
            assert!(
                last_emit_value_c2.lock().unwrap().last_value == last,
                "last emitted value should be {}, but it is {}",
                last,
                last_emit_value_c2.lock().unwrap().last_value
            );
        },
    );

    let check_delay_cnt = Arc::new(Mutex::new(0));
    let check_delay_cnt_cloned = Arc::clone(&check_delay_cnt);

    // Increment counter in separate thread.
    std::thread::spawn(move || {
        for _ in 0..=10 {
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
    let last_emit_value = Arc::new(Mutex::new(CheckFinished {
        last_value: 0,
        completed: false,
    }));
    let last_emit_value_c1 = last_emit_value.clone();
    let last_emit_value_c2 = last_emit_value.clone();

    let o = Subscriber::new(
        move |v| {
            assert!(v >= 0, "integer less than 0 emitted {}", v);
            assert!(v <= 10, "integer greater than 10 emitted {}", v);
        },
        |_observable_error| {},
        move || {},
    );

    let mut s = Observable::new(move |mut o: Subscriber<_>| {
        for i in 0..=last {
            o.next(i);
        }
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    s.subscribe(o);

    let mut s = s.skip(n.try_into().unwrap());

    let o = Subscriber::new(
        move |v| {
            assert!(v > n - 1, "first {} values should be skipped, got {}", n, v);
            if v == last {
                last_emit_value_c1.lock().unwrap().last_value = v;
            }
        },
        |_observable_error| {},
        move || {
            last_emit_value_c2.lock().unwrap().completed = true;
            assert!(
                last_emit_value_c2.lock().unwrap().last_value == last,
                "last emitted value should be {}, but it is {}",
                last,
                last_emit_value_c2.lock().unwrap().last_value
            );
        },
    );

    s.subscribe(o);
    assert!(
        last_emit_value.lock().unwrap().completed,
        "skip operator did not completed observable"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn take_observable() {
    let take_bound = 7_u32;
    let last_emit_value = Arc::new(Mutex::new(CheckFinished {
        last_value: 0,
        completed: false,
    }));
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
            if v == take_bound - 1 {
                last_emit_value_c1.lock().unwrap().last_value = v.try_into().unwrap();
            }
        },
        |_observable_error| {},
        move || {
            last_emit_value_c2.lock().unwrap().completed = true;
            assert!(
                last_emit_value_c2.lock().unwrap().last_value
                    == (take_bound - 1).try_into().unwrap(),
                "last emitted value should be {}, but it is {}",
                take_bound - 1,
                last_emit_value_c2.lock().unwrap().last_value
            );
        },
    );

    let observable = generate_u32_observable(100, move |last_emit_value| {
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

    // Await the thread started in observable.
    if let Err(e) = s.join() {
        // Check if task in observable panicked.
        // If yes, resume and unwind panic to make the test fail with
        // proper error message.
        std::panic::resume_unwind(e);
    };
    assert!(
        last_emit_value.lock().unwrap().completed,
        "take operator did not completed observable"
    );
}

#[test]
fn merge_observable() {
    let values = Arc::new(Mutex::new(Vec::with_capacity(25)));
    let values_cl = Arc::clone(&values);

    let s = Subscriber::on_next(move |v| values_cl.lock().unwrap().push(v));
    let o1 = generate_u32_observable(5, |_| {});
    let o2 = generate_u32_observable(4, |_| {});
    let o3 = generate_u32_observable(4, |_| {});

    let outer = generate_u32_observable(8, |_| {});
    let subscription = outer.merge(vec![o1, o2, o3]).subscribe(s);

    subscription.join().unwrap();
    values.lock().unwrap().sort();
    assert_eq!(
        *values.lock().unwrap(),
        &[0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 6, 7, 8],
        "merge operator failed to merge observable streams"
    );
}

#[test]
fn merge_one_observable() {
    let values = Arc::new(Mutex::new(Vec::with_capacity(18)));
    let values_cl = Arc::clone(&values);

    let s = Subscriber::on_next(move |v| values_cl.lock().unwrap().push(v));

    let outer = generate_u32_observable(8, |_| {});
    let subscription = outer
        .merge_one(generate_u32_observable(8, |_| {}))
        .subscribe(s);

    subscription.join().unwrap();
    values.lock().unwrap().sort();
    assert_eq!(
        *values.lock().unwrap(),
        &[0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8],
        "merge_one operator failed to merge two observable streams"
    );
}
