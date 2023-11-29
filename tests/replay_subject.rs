mod custom_error;
mod register_emissions;

use custom_error::CustomError;
use register_emissions::register_emissions_subscriber;
use rxr::subjects::{BufSize, ReplaySubject};
use rxr::{Observer, Subscribeable};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn replay_subject_emit_than_complete() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = ReplaySubject::emitter_receiver(BufSize::Bounded(5));

    // Emitting a value without any registered subscribers yet; still storing the emitted value.
    stx.next(1);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Register a subscriber and emit the stored value.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 1);
    assert_eq!(nexts.lock().unwrap().last(), Some(&1));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Store emitted value on single subscriber emit.
    stx.next(2);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Store two emitted values with two subsequent emissions to a single subscriber.
    stx.next(3);
    stx.next(4);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 4);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emit all stored values upon registration of additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Store emitted value upon emitting again to the three registered subscribers.
    stx.next(5);

    // Store emitted value upon emitting again to the three registered subscribers.
    // Buffer is full, removing oldest stored value from the buffer to accommodate
    // the new one.
    stx.next(6);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 18);
    assert_eq!(nexts.lock().unwrap().last(), Some(&6));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Signal completion for the ReplaySubject.
    stx.complete();

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 18);
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting values after completion upon registering another subscriber.
    let z = make_subscriber.pop().unwrap()();

    // Emit stored values upon subscription even after completion.
    srx.subscribe(z); // 4th

    // Disregard emissions after completion.
    stx.next(7);
    stx.next(8);
    stx.next(9);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 23); // 18 stored values + 5 stored values
    assert_eq!(completes.lock().unwrap().len(), 4);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn replay_subject_emit_than_error() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();

    let (mut stx, mut srx) = ReplaySubject::emitter_receiver(BufSize::Unbounded);

    // Registering several subscribers.
    srx.subscribe(x); // 1st
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting several values.
    stx.next(1);
    stx.next(2);
    stx.next(3);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Trigger an error on a ReplaySubject.
    stx.error(Arc::new(CustomError));

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // After triggering an error, register another subscriber and emit some values.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th
    stx.next(4);
    stx.next(5);
    stx.next(6);

    // Should emit both, values and an error.
    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 4);
}

#[test]
fn replay_subject_emit_than_complete_time_aware() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();

    // Initializing a ReplaySubject with a time-aware value buffer.
    let (mut stx, mut srx) = ReplaySubject::emitter_receiver_time_aware(BufSize::Bounded(10), 500);

    // Emitting a value without any registered subscribers yet; still
    // storing the emitted value.
    stx.next(1);
    stx.next(2);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Registering a subscriber and emitting the stored value.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Allow time for the time-aware ReplaySubject to remove old values from the buffer.
    std::thread::sleep(Duration::from_millis(700));

    // Registering additional subscribers. No emissions should occur as previously
    // registered values are outdated.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    stx.next(3);

    std::thread::sleep(Duration::from_millis(700));
    stx.next(4);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 8);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Completing the ReplaySubject.
    stx.complete();

    // The emissions after completion are disregarded.
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 8);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // ReplaySubject emits upon subscription even after completion.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 4);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn replay_subject_emit_than_error_time_aware() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();

    // Initializing a ReplaySubject with a time-aware value buffer.
    let (mut stx, mut srx) = ReplaySubject::emitter_receiver_time_aware(BufSize::Bounded(10), 500);

    // Emitting a value without any registered subscribers yet; still
    // storing the emitted values.
    stx.next(1);
    stx.next(2);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Registering a subscriber and emitting the stored value.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Waiting for some time to allow the time-aware ReplaySubject to remove old
    // values from the buffer.
    std::thread::sleep(Duration::from_millis(700));

    // Registering additional subscribers. No emissions should occur as previously
    // registered values are outdated.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    stx.next(3);

    std::thread::sleep(Duration::from_millis(700));
    stx.next(4);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 8);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Trigger an error on the ReplaySubject.
    stx.error(Arc::new(CustomError));

    // The emissions after encountering an error are ignored.
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 8);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // ReplaySubject emits upon subscription even after triggering an error.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 4);
}
