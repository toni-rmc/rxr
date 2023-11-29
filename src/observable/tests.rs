use super::*;

use tokio::time::{sleep, Duration};

pub fn make_emit_u32_observable(
    end: u32,
    last_emit_assert: impl FnMut(u32) + Send + Sync + 'static,
) -> Observable<u32> {
    let last_emit_assert = Arc::new(Mutex::new(last_emit_assert));

    Observable::new(move |mut o: Subscriber<_>| {
        let done = Arc::new(Mutex::new(false));
        let done_c = Arc::clone(&done);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(i) = rx.recv() {
                *done_c.lock().unwrap() = i;
            }
        });

        let last_emit_assert = Arc::clone(&last_emit_assert);
        let jh = std::thread::spawn(move || {
            let mut last_emit = 0;

            for i in 0..=end {
                if *done.lock().unwrap() == true {
                    break;
                }
                last_emit = i;
                o.next(i);
                // Important. Put an await point after each emit.
                std::thread::sleep(Duration::from_millis(1));
            }
            o.complete();
            last_emit_assert.lock().unwrap()(last_emit);
        });

        Subscription::new(
            UnsubscribeLogic::Logic(Box::new(move || {
                if let Err(_) = tx.send(true) {
                    eprintln!("receiver dropped");
                }
            })),
            SubscriptionHandle::JoinThread(jh),
        )
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "reason: this test is unreliable"]
async fn exhaust_map_observable() {
    let observable_occupied: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
    let observable_occupied2 = Arc::clone(&observable_occupied);

    let project_lock = Arc::new(Mutex::new(true));
    let should_finish = Arc::new(Mutex::new(true));
    let should_finish2 = Arc::clone(&should_finish);

    let last_emits_count = Arc::new(Mutex::new(0));
    let last_emits_count2 = Arc::clone(&last_emits_count);

    let global_buffer = Arc::new(Mutex::new(Ok(())));
    {
        let global_buffer_clone = Arc::clone(&global_buffer);

        let o = Subscriber::new(
            |_| {
                // Noop
            },
            |_observable_error| {},
            move || {
                // Set this flag to test next inner observable that should finish.
                // XXX: But this test semaphore sometimes signals that inner observable
                // should emit before exhaust_map actually signals it.
                *observable_occupied2.lock().unwrap() = None;
                *should_finish2.lock().unwrap() = true;
            },
        );

        let outer_o_max_count = 100;
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
                outer_o_max_count
            );
        });

        let should_run = Arc::new(Mutex::new(Vec::<u32>::new()));
        let should_run_clone = Arc::clone(&should_run);
        let did_it_run = Arc::new(Mutex::new(Vec::<u32>::new()));
        let did_it_run_clone = Arc::clone(&did_it_run);

        let lock = Arc::new(Mutex::new(true));

        let mut observable = observable.exhaust_map(move |v| {
            let _project_guard = project_lock.lock().unwrap();
            let global_buffer_clone = Arc::clone(&global_buffer_clone);
            let should_finish_cl = Arc::clone(&should_finish);
            let lock = Arc::clone(&lock);

            // XXX: This part is not sound. Sometimes marks value that should
            // be rejected as the one that should emit and complete and usually value after
            // that one is the one that should emit and complete and this marks it
            // for rejection.
            if observable_occupied.lock().unwrap().is_none() {
                *observable_occupied.lock().unwrap() = Some(v);

                (*should_run_clone.lock().unwrap()).push(v);
            } else {
                *should_finish.lock().unwrap() = false;
            }
            // let observable_occupied3 = Arc::clone(&observable_occupied);

            let did_it_run_clone = Arc::clone(&did_it_run_clone);

            make_emit_u32_observable(inner_o_max_count, move |last_emit_inner_value| {
                let _guard = lock.lock().unwrap();

                // If previous inner observable panicked do not make further checks
                // to prevent global buffer maybe being overwritten with OK(())
                // and losing previous caught panics.
                if global_buffer_clone.lock().unwrap().is_err() {
                    return;
                }

                if *should_finish_cl.lock().unwrap() {
                    *should_finish_cl.lock().unwrap() = false;
                    // This inner observable started emitting. exhaust_map should emit all
                    // of it's values and reject any other inner observable trying
                    // to emit in the mean time.
                    *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                        assert!(
                            last_emit_inner_value == inner_o_max_count,
                            "exhaust_map should emit all values for this inner observable.
Last emitted inner value is {} but it should have reached {}",
                            last_emit_inner_value,
                            inner_o_max_count
                        );
                    });

                    (*did_it_run_clone.lock().unwrap()).push(v);
                } else {
                    // Check that inner observables that started when previous inner observable
                    // was emitting are rejected.
                    *global_buffer_clone.lock().unwrap() = catch_unwind(|| {
                        assert!(
                            last_emit_inner_value < inner_o_max_count,
                            "exhaust_map did not unsubscribed inner observable properly.
It finished emitting inner observable that should be rejected because other inner observable
was emitting it's values. Inner observable reached it's last value {} but should have
been rejected. Outer observable is {}.",
                            last_emit_inner_value,
                            v
                        );
                    });
                }
                // here
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
        sleep(Duration::from_millis(7000)).await;

        assert!(
            *last_emits_count.lock().unwrap() == outer_o_max_count,
            "Outer observable should have emitted {} times, but emitted {} instead",
            outer_o_max_count,
            *last_emits_count.lock().unwrap()
        );

        // Check if exhaust_map emitted more values than it should.
        assert!(
            !((*should_run.lock().unwrap()).len() < (*did_it_run.lock().unwrap()).len()),
            "exhaust_map emitted more values than it should,\nshould = {:?},\ndid = {:?}",
            *should_run.lock().unwrap(),
            *did_it_run.lock().unwrap(),
        );

        // Check if all inner observables that should have run actually did run.
        assert_eq!(
            *should_run.lock().unwrap(),
            *did_it_run.lock().unwrap(),
            "exhaust_map has failed to let some inner observables to run to completion"
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
