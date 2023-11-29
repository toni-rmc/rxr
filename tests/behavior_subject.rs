mod custom_error;
mod register_emissions;

use custom_error::CustomError;
use register_emissions::register_emissions_subscriber;
use rxr::subjects::BehaviorSubject;
use rxr::{Observer, Subscribeable};
use std::sync::Arc;

#[test]
fn behavior_subject_emit_than_complete() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = BehaviorSubject::emitter_receiver(9);

    // Emitting without any registered subscribers yet.
    stx.next(1);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Registering a subscriber and emitting the stored value.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 1);
    assert_eq!(nexts.lock().unwrap().last(), Some(&1));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting once to a single registered subscriber.
    stx.next(2);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 2);
    assert_eq!(nexts.lock().unwrap().last(), Some(&2));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting two more times to a single registered subscriber.
    stx.next(3);
    stx.next(4);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 4);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Registering additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 6);
    assert_eq!(nexts.lock().unwrap().last(), Some(&4));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting two more times to three registered subscribers.
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(nexts.lock().unwrap().last(), Some(&6));
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Completing the BehaviorSubject.
    stx.complete();

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting values after completion upon registering another subscriber.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th
    stx.next(7);
    stx.next(8);
    stx.next(9);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 4);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn behaviour_subject_emit_than_error() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();

    let (mut stx, mut srx) = BehaviorSubject::emitter_receiver(1);

    // Registering subscribers and emitting stored values.
    srx.subscribe(x); // 1st
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 3);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting some values.
    stx.next(1);
    stx.next(2);
    stx.next(3);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Trigger an error on the BehaviorSubject.
    stx.error(Arc::new(CustomError));

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // After triggering an error, register another subscriber and emit some values.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th
    stx.next(4);
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 12);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 4);
}
