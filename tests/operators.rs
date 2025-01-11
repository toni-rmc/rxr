mod generate_observable;

use generate_observable::{
    generate_delayed_observable, generate_u32_observable, generate_u32_observable_async,
};

use std::{
    panic::{catch_unwind, resume_unwind},
    sync::{Arc, Mutex},
    time::Duration,
};

use rxr::{
    subscribe::{Subscriber, Subscription, SubscriptionHandle, UnsubscribeLogic},
    Observable, ObservableExt, Observer, Subscribeable, TimestampedEmit,
};

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

#[tokio::test(flavor = "multi_thread")]
async fn debounce_observable_emit_all() {
    let emitted_values_store = Arc::new(Mutex::new(Vec::with_capacity(29)));
    let emitted_values_store_cl = Arc::clone(&emitted_values_store);

    let o = Subscriber::on_next(move |v: u32| {
        emitted_values_store_cl.lock().unwrap().push(v);
    });

    let end = 28;

    let observable = generate_delayed_observable(end.try_into().unwrap(), 100, |_| {});

    // The debounce duration is shorter than the interval between emissions from the
    // source observable. Thus, every value from the source should be emitted.
    let subscription = observable.debounce(Duration::from_millis(10)).subscribe(o);

    let _ = subscription.join_concurrent().await;

    let stored_cnt = emitted_values_store.lock().unwrap().len();

    assert_eq!(
        stored_cnt, end,
        "debounced observable should have emitted all {end} values, emitted {stored_cnt}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn debounce_observable_emit_one() {
    let emitted_values_store = Arc::new(Mutex::new(Vec::with_capacity(29)));
    let emitted_values_store_cl = Arc::clone(&emitted_values_store);

    let o = Subscriber::on_next(move |v: u32| {
        emitted_values_store_cl.lock().unwrap().push(v);
    });

    let end = 28;

    let observable = generate_delayed_observable(end.try_into().unwrap(), 100, |_| {});

    // The debounce duration is longer than the interval between emissions from the
    // source observable. Only the last value should be emitted upon source completion.
    let subscription = observable
        .debounce(Duration::from_millis(1400))
        .subscribe(o);

    let _ = subscription.join_concurrent().await;

    let stored_cnt = emitted_values_store.lock().unwrap().len();

    assert_eq!(
        stored_cnt, 1,
        "debounced observable should have emitted one last value upon completion, emitted {stored_cnt}"
    );

    let last_value = emitted_values_store.lock().unwrap().pop();

    match last_value {
        Some(last_value) => {
            assert_eq!(
                last_value, end - 1,
                "the last value emitted by the debounced observable upon completion should be {}, not {last_value}",
                end - 1
            );
        }
        None => unreachable!(),
    }
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

#[test]
fn timestamped_emit() {
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
        for i in 0..last + 1 {
            o.next(i);
        }
        o.complete();
        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    });

    s.subscribe(o);

    let mut s = s.timestamp();

    let o = Subscriber::new(
        move |v: TimestampedEmit<i32>| {
            assert!(
                v.timestamp > 0,
                "timestamped value should have timestamp greater than 0, got {}",
                v.timestamp
            );
            assert!(
                v.value >= 0,
                "timestamped value should have value greater than 0, got {}",
                v.value
            );
            if v.value == last {
                last_emit_value_c1.lock().unwrap().last_value = v.value;
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
        "timestamp operator did not completed observable"
    );
}

#[test]
fn first_observable() {
    let emitted = Arc::new(Mutex::new(vec![]));
    let emitted_cl = Arc::clone(&emitted);
    let completed = Arc::new(Mutex::new(false));
    let completed_cl = Arc::clone(&completed);

    let mut observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    observer.on_complete(move || *completed_cl.lock().unwrap() = true);
    let observable = generate_u32_observable(17, |_| {});

    // Predicate will return `true`.
    let mut o = observable.first(|v, _| v > 8, None);
    let s = o.subscribe(observer);
    let _ = s.join();

    let emitted = emitted.lock().unwrap();
    let emitted: &[u32] = emitted.as_ref();

    assert!(
        emitted.len() > 0,
        "first operator failed to emit first value"
    );
    assert_eq!(
        emitted,
        &[9],
        "first operator failed to emit correct first value"
    );
    assert!(
        *completed.lock().unwrap() == true,
        "first operator failed to complete"
    );

    let emitted = Arc::new(Mutex::new(vec![]));
    let emitted_cl = Arc::clone(&emitted);

    // Reset `completed` flag.
    *completed.lock().unwrap() = false;
    let completed_cl = Arc::clone(&completed);
    let mut observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    observer.on_complete(move || *completed_cl.lock().unwrap() = true);
    let observable = generate_u32_observable(14, |_| {});

    // Predicate will not return `true`, default value provided.
    let mut o = observable.first(|v, _| v > 100, Some(20));
    let s = o.subscribe(observer);
    let _ = s.join();

    let emitted = emitted.lock().unwrap();
    let emitted: &[u32] = emitted.as_ref();

    assert_eq!(
        emitted,
        &[20],
        "first operator failed to emit default value"
    );
    assert!(
        *completed.lock().unwrap() == true,
        "first operator failed to complete after taking default value"
    );

    use rxr::observable::EmptyError;

    let emitted = Arc::new(Mutex::new(vec![]));
    let emitted_cl = Arc::clone(&emitted);

    // Reset `completed` flag.
    *completed.lock().unwrap() = false;
    let completed_cl = Arc::clone(&completed);
    let errored = Arc::new(Mutex::new(None));
    let errored_cl = Arc::clone(&errored);

    let mut observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    observer.on_complete(move || *completed_cl.lock().unwrap() = true);
    observer.on_error(move |err| {
        // Check error type.
        match err.downcast_ref::<EmptyError>() {
            Some(e) => e,
            None => unreachable!("first operator emitted wrong error type"),
        };
        *errored_cl.lock().unwrap() = Some(err);
    });

    let observable = generate_u32_observable(14, |_| {});

    // Predicate will not return `true`, default value is not provided.
    let mut o = observable.first(|v, _| v > 100, None);
    let s = o.subscribe(observer);
    let _ = s.join();

    let emitted = emitted.lock().unwrap();
    let emitted: &[u32] = emitted.as_ref();

    assert_eq!(
        emitted,
        &[],
        "first operator with no matches and no default value emitted a value when it shouldn't have"
    );
    assert!(
        *completed.lock().unwrap() == false,
        "first operator with no matches and no default value completed when it shouldn't have"
    );
    assert!(
        errored.lock().unwrap().is_some(),
        "first operator with no matches and no default value did not emitted error"
    );
}

#[test]
fn scan_observable() {
    let emitted = Arc::new(Mutex::new(Vec::with_capacity(8)));
    let emitted_cl = Arc::clone(&emitted);
    let observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    let observable = generate_u32_observable(7, |_| {});

    let mut o = observable.scan(|total: u32, v: u32| total + v, None);
    let s = o.subscribe(observer);
    let _ = s.join();

    let emitted = emitted.lock().unwrap();
    let emitted: &[u32] = emitted.as_ref();

    assert_eq!(
        emitted,
        &[0, 1, 3, 6, 10, 15, 21, 28],
        "scan operator without initial seed value failed to correctly accumulate emitted values"
    );

    let emitted = Arc::new(Mutex::new(Vec::with_capacity(8)));
    let emitted_cl = Arc::clone(&emitted);
    let observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    let observable = generate_u32_observable(7, |_| {});

    let mut o = observable.scan(|total: u32, v: u32| total + v, Some(20));
    let s = o.subscribe(observer);
    let _ = s.join();

    let emitted = emitted.lock().unwrap();
    let emitted: &[u32] = emitted.as_ref();

    assert_eq!(
        emitted,
        &[20, 21, 23, 26, 30, 35, 41, 48],
        "scan operator with initial seed value failed to correctly accumulate emitted values"
    );
}

#[test]
fn zip_observable() {
    let emitted = Arc::new(Mutex::new(Vec::with_capacity(7)));
    let emitted_cl = Arc::clone(&emitted);
    let observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    let outer = generate_u32_observable(7, |_| {});

    let inner = generate_u32_observable(6, |_| {});

    let mut z = outer.zip(vec![inner]);
    let s = z.subscribe(observer);
    let _ = s.join();

    assert_eq!(
        <Vec<Vec<u32>> as AsRef<Vec<Vec<u32>>>>::as_ref(&emitted.lock().unwrap().as_ref()),
        &[
            &[0, 0],
            &[1, 1],
            &[2, 2],
            &[3, 3],
            &[4, 4],
            &[5, 5],
            &[6, 6]
        ],
        "zip operator failed to zip one observable"
    );

    let emitted = Arc::new(Mutex::new(Vec::with_capacity(7)));
    let emitted_cl = Arc::clone(&emitted);
    let observer = Subscriber::on_next(move |v| {
        emitted_cl.lock().unwrap().push(v);
    });
    let outer = generate_u32_observable(7, |_| {}).map(|v| format!("s: {}", v));

    let inner = generate_u32_observable(6, |_| {}).map(|v| format!("1st: {}", v));
    let inner2 = generate_u32_observable(8, |_| {}).map(|v| format!("2nd: {}", v));
    let inner3 = generate_u32_observable(4, |_| {}).map(|v| format!("3rd: {}", v));

    let mut z = outer.zip(vec![inner, inner2, inner3]);
    let s = z.subscribe(observer);
    let _ = s.join();

    assert_eq!(
        <Vec<Vec<String>> as AsRef<Vec<Vec<String>>>>::as_ref(&emitted.lock().unwrap().as_ref()),
        &[
            &["s: 0", "1st: 0", "2nd: 0", "3rd: 0"],
            &["s: 1", "1st: 1", "2nd: 1", "3rd: 1"],
            &["s: 2", "1st: 2", "2nd: 2", "3rd: 2"],
            &["s: 3", "1st: 3", "2nd: 3", "3rd: 3"],
            &["s: 4", "1st: 4", "2nd: 4", "3rd: 4"]
        ],
        "zip operator failed to zip multiple observables"
    );
    emitted.lock().unwrap().clear();
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
fn take_until_observable_unsubscribe_notifier() {
    let observable_result = Arc::new(Mutex::new(Ok(())));
    let notifier_result = Arc::new(Mutex::new(Ok(())));

    // Open inner scope to shorten the lifetimes of observable_result_clone and notifier_result_clone.
    {
        let observable_result_clone = Arc::clone(&observable_result);
        let notifier_result_clone = Arc::clone(&notifier_result);
        let outer_o_max_count = 100;
        let inner_o_max_count = 20;

        // Chaining the notifier observable with `delay()` to allow the main observable
        // sufficient time to emit some values before notifier cancels subscription.
        let notifier =
            generate_u32_observable(inner_o_max_count, move |notifier_last_emit_value| {
                // Check the last value emitted by the notifier observable.
                *notifier_result_clone.lock().unwrap() = catch_unwind(|| {
                    assert!(
                        notifier_last_emit_value < inner_o_max_count,
                        "notifier should have been stopped emitting all of its values by `take_until()`.
Emitted {} which is its last value", notifier_last_emit_value 
                );
                });
            })
            .delay(40);

        let subscriber = Subscriber::on_next(|_| {});

        let observable = generate_u32_observable(outer_o_max_count, move |last_emit_value| {
            // Check the last value emitted by the observable.
            *observable_result_clone.lock().unwrap() = catch_unwind(|| {
                assert!(
                    last_emit_value < outer_o_max_count,
                    "observable should have been stopped emitting all of its values by notifier.
Emitted {} which is its last value",
                    last_emit_value
                );
            });
        });

        // Pass `true` to `take_until()` so outer observable unsubscribes notifier too.
        let subscription = observable.take_until(notifier, true).subscribe(subscriber);

        // Wait for outer observable to complete.
        if let Err(e) = subscription.join() {
            resume_unwind(e);
        }
    }
    // Give some time to make sure notifier observable is finished if not stopped.
    std::thread::sleep(Duration::from_millis(4000));

    let m = Arc::into_inner(notifier_result).unwrap();
    let m = m.into_inner().unwrap();

    if let Err(e) = m {
        resume_unwind(e);
    };

    let m = Arc::into_inner(observable_result).unwrap();
    let m = m.into_inner().unwrap();

    if let Err(e) = m {
        resume_unwind(e);
    };
}

#[tokio::test(flavor = "multi_thread")]
async fn take_until_observable_do_not_unsubscribe_notifier() {
    let observable_result = Arc::new(Mutex::new(Ok(())));
    let notifier_result = Arc::new(Mutex::new(Ok(())));

    // Open inner scope to shorten the lifetimes of observable_result_clone and notifier_result_clone.
    {
        let observable_result_clone = Arc::clone(&observable_result);
        let notifier_result_clone = Arc::clone(&notifier_result);
        let outer_o_max_count = 100;
        let inner_o_max_count = 40;

        // Chaining the notifier observable with `delay()` to allow the main observable
        // sufficient time to emit some values before notifier cancels subscription.
        let notifier =
            generate_u32_observable_async(inner_o_max_count, move |notifier_last_emit_value| {
                // Check the last value emitted by the notifier observable.
                *notifier_result_clone.lock().unwrap() = catch_unwind(|| {
                    assert!(
                        notifier_last_emit_value == inner_o_max_count,
                        "notifier should have emit all of its values.
Emitted {} which is not its last value",
                        notifier_last_emit_value
                    );
                });
            })
            .await
            .delay(20);

        let subscriber = Subscriber::on_next(|_| {});

        let observable = generate_u32_observable_async(outer_o_max_count, move |last_emit_value| {
            // Check the last value emitted by the observable.
            *observable_result_clone.lock().unwrap() = catch_unwind(|| {
                assert!(
                    last_emit_value < outer_o_max_count,
                    "observable should have been stopped emitting all of its values by notifier.
Emitted {} which is its last value",
                    last_emit_value
                );
            });
        })
        .await;

        // Using `false` with `take_until()` to avoid unsubscribing the notifier observable.
        let subscription = observable.take_until(notifier, false).subscribe(subscriber);

        // Wait for outer observable to complete.
        if let Err(e) = subscription.join_concurrent().await {
            resume_unwind(e);
        }
    }
    // Give some time to make sure notifier observable is finished if not stopped.
    tokio::time::sleep(Duration::from_millis(4000)).await;

    let m = Arc::into_inner(notifier_result).unwrap();
    let m = m.into_inner().unwrap();

    if let Err(e) = m {
        resume_unwind(e);
    };

    let m = Arc::into_inner(observable_result).unwrap();
    let m = m.into_inner().unwrap();

    if let Err(e) = m {
        resume_unwind(e);
    };
}

#[test]
fn take_until_observable_with_subject_notifier() {
    let observable_result = Arc::new(Mutex::new(Ok(())));

    // Open inner scope to shorten the lifetime of observable_result_clone.
    {
        let observable_result_clone = Arc::clone(&observable_result);
        let outer_o_max_count = 100;

        let subscriber = Subscriber::on_next(|_| {});

        let observable = generate_u32_observable(outer_o_max_count, move |last_emit_value| {
            // Check the last value emitted by the observable.
            *observable_result_clone.lock().unwrap() = catch_unwind(|| {
                assert!(
                    last_emit_value < outer_o_max_count,
                    "observable should have been stopped emitting all of its values by notifier.
Emitted {} which is its last value",
                    last_emit_value
                );
            });
        });
        let (mut emitter, notifier) = rxr::subjects::Subject::<()>::emitter_receiver();

        // Pass `false` to `take_until()` so it doesn't unsubscribe the subject.
        let subscription = observable
            .take_until(notifier.into(), false)
            .subscribe(subscriber);

        // Give some time to make sure observable can emit some values before unsubscribing.
        std::thread::sleep(Duration::from_millis(10));

        // Emitting a value to signal the notifier to cease observable emissions.
        emitter.next(());

        // Wait for outer observable to complete.
        if let Err(e) = subscription.join() {
            resume_unwind(e);
        }
    }

    let m = Arc::into_inner(observable_result).unwrap();
    let m = m.into_inner().unwrap();

    if let Err(e) = m {
        resume_unwind(e);
    };
}

#[test]
fn take_last_observable() {
    let values = Arc::new(Mutex::new(Vec::with_capacity(25)));
    let values_cl = Arc::clone(&values);
    let values_cl2 = Arc::clone(&values);
    let values_cl3 = Arc::clone(&values);
    let values_cl4 = Arc::clone(&values);

    let s = Subscriber::on_next(move |v| values_cl.lock().unwrap().push(v));
    let o = generate_u32_observable(100, |_| {});

    let subscription = o.take_last(4).subscribe(s);

    subscription.join().unwrap();

    // Take some last values.
    assert_eq!(
        *values.lock().unwrap(),
        &[97, 98, 99, 100],
        "take_last operator failed to correctly retrieve the last values"
    );

    let s = Subscriber::on_next(move |v| values_cl2.lock().unwrap().push(v));
    let o = generate_u32_observable(20, |_| {});

    let subscription = o.take_last(0).subscribe(s);

    subscription.join().unwrap();

    // Do not take any values.
    assert_eq!(
        *values.lock().unwrap(),
        &[97, 98, 99, 100],
        "take_last operator failed to correctly retrieve no values"
    );

    let s = Subscriber::on_next(move |v| values_cl3.lock().unwrap().push(v));
    let o = generate_u32_observable(20, |_| {});

    let subscription = o.take_last(1).subscribe(s);

    subscription.join().unwrap();

    // Take one value.
    assert_eq!(
        *values.lock().unwrap(),
        &[97, 98, 99, 100, 20],
        "take_last operator failed to correctly retrieve the one last value"
    );

    let s = Subscriber::on_next(move |v| values_cl4.lock().unwrap().push(v));
    let o = generate_u32_observable(4, |_| {});

    let subscription = o.take_last(5).subscribe(s);

    subscription.join().unwrap();

    // Take all values.
    assert_eq!(
        *values.lock().unwrap(),
        &[97, 98, 99, 100, 20, 0, 1, 2, 3, 4],
        "take_last operator failed to correctly retrieve all values"
    );
}

#[test]
fn take_while_observable() {
    let values = Arc::new(Mutex::new(Vec::with_capacity(10)));
    let values_cl = Arc::clone(&values);

    let s = Subscriber::on_next(move |v| values_cl.lock().unwrap().push(v));
    let o = generate_u32_observable(25, |last_emit_value| {
        assert!(
            last_emit_value < 10,
            "take_while failed to unsubscribe background emissions, last emitted value is {} but should be less than 10",
            last_emit_value
        );
    });

    let subscription = o
        .take_while(|v| {
            if v < &6 {
                return true;
            }
            false
        })
        .subscribe(s);

    if let Err(e) = subscription.join() {
        // Check if task in observable panicked.
        // If yes, resume and unwind panic to make the test fail with
        // proper error message.
        std::panic::resume_unwind(e);
    };

    assert_eq!(
        *values.lock().unwrap(),
        &[0, 1, 2, 3, 4, 5],
        "take_while operator failed to emit last values correctly"
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
