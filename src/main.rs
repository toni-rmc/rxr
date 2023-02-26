use std::{marker::PhantomData, any::Any};

pub struct Subject {
    subscriber: Option<Box<dyn Any>>, // Option<T> maybe
}

impl Subject {
    pub fn foo<N: 'static>(self) {
        // // // if let Some(s) = self.subscriber.downcast_ref::<Subscriber<N>>() {
        
       // }
    }
}

pub trait Observer<T> {
   fn next(&mut self, v: T); 
}

pub struct Obs<T> {
        next_fn: Box<dyn FnMut(T)>,
}   

impl<T> Observer<T> for Obs<T> {
    fn next(&mut self, v: T) {
            (self.next_fn)(v);
    }
}

pub fn observe<N>(next_fnc: impl FnMut(N) + 'static) -> Obs<N> {
    Obs {
        next_fn: Box::new(next_fnc)
    }
}

pub trait Observable<N> {
    fn subscribe(self, observer: Obs<N>) -> Subject;

    fn pipe(self, oo: &mut [impl FnMut(Self) -> Self]) -> Self where Self: Sized {
        let mut t = self;
        for i in oo {
            t = i(t);
        }
        t
    }
}

struct BaseObservable<N> {
    output_function: Box<dyn FnMut(Obs<N>)>,
}

impl<N> Observable<N> for BaseObservable<N> {
    fn subscribe(mut self, observer: Obs<N>) -> Subject {
        (self.output_function)(observer);
        Subject { subscriber: None }
    }
}

// TODO: Add subscribe method to Subscriber
// and inner observable field `output_observable`
pub struct Subscriber<N, O: Observable<N>> {
    output_observable: Option<O>,
    phantom: PhantomData<N>,
}

impl<N> Subscriber<N, BaseObservable<N>> {
    pub fn new(output_fn: Box<dyn FnMut(Obs<N>) + Send + 'static>) -> Subscriber<N, BaseObservable<N>> {
        Subscriber { output_observable: Some(BaseObservable { output_function: output_fn }), phantom: PhantomData }
    }
}

impl<N, O: Observable<N>> Subscriber<N, O> { 
    pub fn pipe(mut self, observables: &mut [impl FnMut(O) -> O]) -> Self {
        let mut o = self.output_observable.unwrap();

        for i in observables {
            o = i(o);
        }
        
        self.output_observable = Some(o);
        self
    }
}

impl<N: 'static, O: Observable<N> + 'static> Observable<N> for Subscriber<N, O> {
    fn subscribe(mut self, observer: Obs<N>) -> Subject {
        let oo = self.output_observable.take().unwrap();

        let mut subject = oo.subscribe(observer);
        // self.output_observable = Some(oo);
        subject.subscriber = Some(Box::new(self));
        subject
    }
}

// pub trait OutputObservable<V>: Observable<V> {
//     type OutputFn;
// }
// 
// impl<T> OutputObservable<T> for Subscriber<T> {
//     type OutputFn = Box<dyn FnMut(dyn Observer<NexVal = T, NextFn = Box<dyn FnMut(T)>>)>;
//     //type OutputFn = Box<dyn FnMut(T)>;
// }

pub struct Map<T: Observable<V>, V, R> {
    inner_obs: T,
    transform_fn: fn(V) -> R,
}

pub fn map<T: Observable<V>, V, R>(mapfn: fn(V) -> R) -> impl FnMut(T) -> Map<T, V, R> {
     move |h: T| -> Map<T, V, R> {
        Map { inner_obs: h, transform_fn: mapfn, }
    }
}

impl<T: Observable<V>, V: 'static, R: 'static> Observable<R> for Map<T, V, R> {

    fn subscribe(self, mut observer: Obs<R>) -> Subject {

        let map_observer = observe(move |i: V| {
            let v = (self.transform_fn)(i);
            observer.next(v);

        });
        self.inner_obs.subscribe(map_observer)
    }
}

// impl ObservableOperator for Map {
// }

fn main() {
    let s = Subscriber::new(Box::new(|mut observer| {
        for i in 0..10 {
            observer.next(i);
        }
    }));

    // let v1 = vec![Box::new(map::<BaseObservable<i32>, i32, i32>(|i| { i + 1000}))];

    s.subscribe(observe(|v| {
        println!("{}", v);
    }));
}
