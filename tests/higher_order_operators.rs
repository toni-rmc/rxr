mod generate_observable;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use generate_observable::generate_u32_observable;
use rxr::subscribe::Subscriber;
use rxr::{ObservableExt, Subscribeable};
use tokio::time::sleep;

#[test]
fn switch_map_observable() {
    let last_emits_count = Arc::new(Mutex::new(0_u32));
    let last_emits_count2 = Arc::clone(&last_emits_count);
    let inner_emits_cnt = Arc::new(Mutex::new(0_u32));
    let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt);
    let global_buffer = Arc::new(Mutex::new(Ok(())));
    {
        let global_buffer_clone = Arc::clone(&global_buffer);

        let o = Subscriber::new(
            |_| {
                // Noop
            },
            |_observable_error| {},
            || {},
        );

        let outer_o_max_count = 100;
        let inner_o_max_count = 10;

        use std::panic::catch_unwind;

        let observable = generate_u32_observable(outer_o_max_count, move |last_emit_value| {
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

            generate_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
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
{} which is its last value",
                            v,
                            last_emit_inner_value
                        );
                    });
                } else {
                    // Outer observable emitted value is outer_o_max_count which is last value and
                    // switch_map should not unsubscribe it consequently last inner
                    // observable subscribed should emit all its values.
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
                            "last inner observable should have emitted all of its values.
Expected {}, found {}",
                            inner_o_max_count,
                            last_emit_inner_value
                        );
                    });
                }
            })
        });

        let s = observable.subscribe(o);

        // Await the thread started in outer observable.
        if let Err(e) = s.join() {
            // Check if task in outer observable panicked.
            //if e.is_panic() {
            // If yes, resume and unwind panic to make the test fail with
            // proper error message.
            std::panic::resume_unwind(e);
            //}
        };

        // Give some time to make sure all inner observables are finished.
        std::thread::sleep(Duration::from_millis(3000));

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

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concat_map_observable() {
    use std::panic::catch_unwind;

    let compare_outer_values = Arc::new(Mutex::new(0_u32));
    let compare_outer_values2 = Arc::clone(&compare_outer_values);
    let lock = Arc::new(Mutex::new(true));

    // Flag for determining are inner observables emitting in sequential order or not.
    let ongoing_emissions = Arc::new(Mutex::new(false));
    let ongoing_emissions2 = Arc::clone(&ongoing_emissions);

    let global_buffer = Arc::new(Mutex::new(Ok(())));
    {
        let global_buffer_clone = Arc::clone(&global_buffer);
        let global_buffer_clone2 = Arc::clone(&global_buffer);

        let sequence_guard = Arc::new(Mutex::new(true));
        let o = Subscriber::new(
            move |v: u32| {
                if v == 0 {
                    let _sequence_guard = sequence_guard.lock().unwrap();
                    let ongoing_emissions3 = Arc::clone(&ongoing_emissions);

                    // Protect global_buffer from overwriting Err() with Ok().
                    if global_buffer_clone2.lock().unwrap().is_ok() {
                        *global_buffer_clone2.lock().unwrap() = catch_unwind(move || {
                            assert!(
                                *ongoing_emissions3.lock().unwrap() == false,
                                "concat_map started emitting next inner observable while previous one still not completed"
                            );
                        });
                        *ongoing_emissions.lock().unwrap() = true;
                    }
                }
            },
            |_observable_error| {},
            move || {
                *ongoing_emissions2.lock().unwrap() = false;
            },
        );

        let outer_o_max_count = 100;
        let inner_o_max_count = 10;

        let observable = generate_u32_observable(outer_o_max_count, move |last_emit_value| {
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

            generate_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
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
                    // Every inner observable should finish emitting all of its values.
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
        if let Err(e) = s.join() {
            // Check if task in outer observable panicked.
            // if e.is_panic() {
            // If yes, resume and unwind panic to make the test fail with
            // proper error message.
            std::panic::resume_unwind(e);
            // }
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
            values_emitted_count, outer_o_max_count,
            "concat_map has failed to project all of the inner observables.
Projected {}, should project {}",
            values_emitted_count, outer_o_max_count
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
            |_| {
                // Noop
            },
            |_observable_error| {},
            || {},
        );

        let outer_o_max_count = 100;
        let inner_o_max_count = 10;

        use std::panic::catch_unwind;

        let observable = generate_u32_observable(outer_o_max_count, move |last_emit_value| {
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

        let mut observable = observable.merge_map(move |_| {
            let global_buffer_clone = Arc::clone(&global_buffer_clone);
            let inner_emits_cnt2 = Arc::clone(&inner_emits_cnt2);
            let lock = Arc::clone(&lock);

            generate_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
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
                        "inner observable should have emitted all of its values.
Expected {}, found {}",
                        inner_o_max_count,
                        last_emit_inner_value
                    );
                });
            })
        });

        let s = observable.subscribe(o);

        // Await the task started in outer observable.
        if let Err(e) = s.join() {
            // Check if task in outer observable panicked.
            // if e.is_panic() {
            // If yes, resume and unwind panic to make the test fail with
            // proper error message.
            std::panic::resume_unwind(e);
            // }
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
