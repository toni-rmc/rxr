use std::{error::Error, rc::Rc};

pub trait Observer {
    type NextFnType;

    fn next(&mut self, _: Self::NextFnType);
    fn complete(&mut self);
    fn error(&mut self, _: Rc<dyn Error>);
}
