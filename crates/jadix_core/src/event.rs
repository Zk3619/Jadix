pub struct Handler<T = ()>(pub Box<dyn FnMut(T)>);

impl<T> Handler<T> {
    pub fn new(callback: impl FnMut(T) + 'static) -> Self {
        Self(Box::new(callback))
    }
    pub fn call(&mut self, argument: T) {
        (self.0)(argument);
    }
}

impl<T, Callback: FnMut(T) + 'static> From<Callback> for Handler<T> {
    fn from(callback: Callback) -> Self {
        Self::new(callback)
    }
}

pub struct ZeroArgFn<Callback>(pub Callback);

impl<Callback: FnMut() + 'static> From<ZeroArgFn<Callback>> for Handler<()> {
    fn from(mut callback: ZeroArgFn<Callback>) -> Self {
        Self::new(move |_| (callback.0)())
    }
}

impl Default for Handler<()> {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl From<Box<dyn FnMut()>> for Handler<()> {
    fn from(mut callback: Box<dyn FnMut()>) -> Self {
        Self::new(move |_| callback())
    }
}
