/**
 * This `Observable` waits for user input and emits both a value and a completion
 * signal upon success. In case of any errors, it signals them to the attached `Observer`.
 *
 * Ensure errors are wrapped in an `Arc` before passing them to the Observer's
 * `error` function.
 */

use std::{error::Error, fmt::Display, io, sync::Arc};

use rxr::{subscribe::*, Observable, Observer, Subscribeable};

#[derive(Debug)]
struct MyErr(i32);

impl Display for MyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "number should be less than 100, you entered {}", self.0)
    }
}

impl Error for MyErr {}

// Creates an `Observable<i32>` that processes user input and emits or signals errors.
pub fn get_less_than_100() -> Observable<i32> {
    Observable::new(|mut observer| {
        let mut input = String::new();

        println!("Please enter an integer (less than 100):");

        if let Err(e) = io::stdin().read_line(&mut input) {
            // Send input error to the observer.
            observer.error(Arc::new(e));
            return Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil);
        }

        match input.trim().parse::<i32>() {
            Err(e) => {
                // Send parsing error to the observer.
                observer.error(Arc::new(e));
            }
            Ok(num) if num > 100 => {
                // Send custom error to the observer.
                observer.error(Arc::new(MyErr(num)))
            }
            Ok(num) => {
                // Emit the parsed value to the observer.
                observer.next(num);
            }
        }

        // Signal completion if there are no errors.
        // Note: `complete` does not affect the outcome if `error` was called before it.
        observer.complete();

        Subscription::new(UnsubscribeLogic::Nil, SubscriptionHandle::Nil)
    })
}

fn main() {
    let observer = Subscriber::new(
        |input| println!("You entered: {}", input),
        Some(|e| eprintln!("{}", e)),
        Some(|| println!("User input handled")),
    );

    let mut observable = get_less_than_100();

    observable.subscribe(observer);
}
