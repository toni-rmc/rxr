mod custom_error;
mod register_emissions;

use custom_error::CustomError;
use register_emissions::register_emissions_subscriber;
use rxr::subjects::AsyncSubject;
use rxr::{Observer, Subscribeable};
use std::sync::Arc;

#[test]
fn async_subject_complete() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = AsyncSubject::emitter_receiver();

    // Capture the initial value within the AsyncSubject.
    stx.next(1);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Subscribing a new observer.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Update the stored value.
    stx.next(2);

    // Register two additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting twice more to each of the three registered subscribers.
    stx.next(5);
    stx.next(6);

    // No values are emitted as the AsyncSubject hasn't completed.
    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Signal completion of the AsyncSubject.
    stx.complete();
    stx.next(7);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 3);
    assert_eq!(nexts.lock().unwrap().last(), Some(&6));
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Subscribe and emit the stored value after completion.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 4th
    srx.subscribe(z); // 5th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 5);
    assert_eq!(nexts.lock().unwrap().last(), Some(&6));
    assert_eq!(completes.lock().unwrap().len(), 5);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn async_subject_complete_empty() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = AsyncSubject::emitter_receiver();

    // Subscribing a new observer.
    srx.subscribe(x); // 1st

    // Adding additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Signal completion for the AsyncSubject.
    stx.complete();

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 3);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Disregard subsequent `next()` calls.
    stx.next(7);

    // Subscribe and complete after completion without prior registration.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 4);
    assert_eq!(errors.lock().unwrap().len(), 0);
}

#[test]
fn async_subject_error() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = AsyncSubject::emitter_receiver();

    // Capturing the initial value within the AsyncSubject.
    stx.next(1);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Subscribing a new observer.
    srx.subscribe(x); // 1st

    assert_eq!(srx.len(), 1);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Update the stored value.
    stx.next(2);

    // Registering additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Emitting two more times to each of the 3 registered subscribers.
    stx.next(5);
    stx.next(6);

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Trigger an error on the AsyncSubject.
    stx.error(Arc::new(CustomError));
    stx.next(7);

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // Subscribe and emit stored value after encountering an error.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 4th
    srx.subscribe(z); // 5th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 5);
}

#[test]
fn async_subject_error_empty() {
    let (mut make_subscriber, nexts, completes, errors) = register_emissions_subscriber();

    let x = make_subscriber.pop().unwrap()();
    let (mut stx, mut srx) = AsyncSubject::emitter_receiver();

    // Subscribing a new observer.
    srx.subscribe(x); // 1st

    // Adding additional subscribers.
    let y = make_subscriber.pop().unwrap()();
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(y); // 2nd
    srx.subscribe(z); // 3rd

    assert_eq!(srx.len(), 3);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 0);

    // Trigger an error on the AsyncSubject.
    stx.error(Arc::new(CustomError));

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 3);

    // Disregard subsequent `next()` calls.
    stx.next(7);

    // Subscribe after AsyncSubject triggers an error.
    let z = make_subscriber.pop().unwrap()();
    srx.subscribe(z); // 4th

    assert_eq!(srx.len(), 0);
    assert_eq!(nexts.lock().unwrap().len(), 0);
    assert_eq!(nexts.lock().unwrap().last(), None);
    assert_eq!(completes.lock().unwrap().len(), 0);
    assert_eq!(errors.lock().unwrap().len(), 4);
}
