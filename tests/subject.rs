mod custom_error;
mod register_emissions;

use custom_error::CustomError;
use register_emissions::register_emissions_subscriber;
use rxr::subjects::Subject;
use rxr::{Observer, Subscribeable};
use std::sync::Arc;

#[test]
fn subject_emit_than_complete() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = Subject::emitter_receiver();

    // Emitting a value, but there are currently no registered subscribers.
    stx.next(1);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Register subscriber.
    srx.subscribe(x); // 1st

    // Emissions are not stored so nothing is emitted after subscribing.
    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting a single value to a single registered subscriber.
    stx.next(2);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 1);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting two additional values to the same registered subscriber.
    stx.next(3);
    stx.next(4);

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 3);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Register 2 additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 3);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting two more times to each of the 3 registered subscribers.
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Complete Subject.
    stx.complete();

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Register another subscriber and emit some values after completion.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th
    stx.next(7);
    stx.next(8);
    stx.next(9);

    // Subject stops emitting values after completion.
    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 4);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn subject_emit_than_error() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();

    let (mut stx, mut srx) = Subject::emitter_receiver();

    // Register some subscribers.
    srx.subscribe(x); // 1st
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    // Emit values.
    stx.next(1);
    stx.next(2);
    stx.next(3);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Triggering an error on a Subject.
    stx.error(Arc::new(CustomError));

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // After encountering an error, registering a new subscriber and emitting values.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th
    stx.next(4);
    stx.next(5);
    stx.next(6);

    // Subject halts value emissions upon encountering an error.
    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 9);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 4);
}
