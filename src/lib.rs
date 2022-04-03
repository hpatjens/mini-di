use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex},
};

#[cfg(test)]
mod tests;

type Constructor = Arc<dyn for<'r> Fn(&'r Resolver) -> Box<dyn Any> + Send + Sync>;

pub struct Container<P: GetConstructor = ()> {
    parent: P,
    constructors: BTreeMap<TypeId, Constructor>,
}

impl Container {
    /// Create a new `Container` with no parent container
    pub fn new() -> Self {
        Self {
            parent: (),
            constructors: Default::default(),
        }
    }

    /// Create a new `Container` with the given `parent` `Container`
    pub fn with_parent<P: GetConstructor>(parent: P) -> Container<P> {
        Container {
            parent,
            constructors: Default::default(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    AlreadyRegistered,
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Registration<'container, R, P: GetConstructor> {
    _phantom: PhantomData<R>,
    container: &'container mut Container<P>,
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: Sync + Send + 'static + Clone,
    P: GetConstructor,
{
    pub fn clone(self, value: R) -> Result<()> {
        let value = Box::new(value);
        let constructor = Arc::new(move |_: &Resolver| value.clone() as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static + Construct,
    P: GetConstructor,
{
    pub fn construct_it(self) -> Result<()> {
        let constructor =
            Arc::new(move |locator: &Resolver| Box::new(R::construct(locator)) as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static,
    P: GetConstructor,
{
    pub fn construct<E>(self) -> Result<()>
    where
        E: 'static + ConstructAs<Target = R>,
    {
        let constructor = Arc::new(move |locator: &Resolver| {
            let new = Box::new(E::construct_as(locator));
            new as Box<dyn Any>
        });
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static,
    P: GetConstructor,
{
    pub fn construct_with<F>(self, constructor: F) -> Result<()>
    where
        F: Fn(&Resolver) -> R + Send + Sync + 'static,
    {
        let constructor =
            Arc::new(move |resolver: &Resolver| Box::new((constructor)(resolver)) as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<P: GetConstructor> Container<P> {
    #[must_use]
    pub fn when<R>(&mut self) -> Registration<R, P> {
        Registration {
            _phantom: PhantomData,
            container: self,
        }
    }

    fn register_constructor<T: 'static>(&mut self, constructor: Constructor) -> Result<()> {
        match self.constructors.insert(TypeId::of::<T>(), constructor) {
            Some(_) => Err(Error::AlreadyRegistered),
            None => Ok(()),
        }
    }

    // Register the type `T` to be constructed when it is needed and an `Rc` is given out upon calling `resolve`.
    pub fn register_singleton<T: Construct + Send + Sync + 'static>(&mut self) -> Result<()> {
        let singleton: Mutex<Option<Arc<T>>> = Mutex::new(None);
        let constructor = Arc::new(move |locator: &Resolver| {
            if let Some(arc) = &*singleton.lock().unwrap() {
                return Box::new(arc.clone()) as Box<dyn Any>;
            }
            let value = Arc::new(T::construct(locator));
            *singleton.lock().unwrap() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.register_constructor::<Arc<T>>(constructor)
    }

    /// Get a `Resolver` that borrows the `Container`
    pub fn as_resolver(&self) -> Resolver {
        Resolver(self)
    }
}

pub struct Resolver<'r>(&'r dyn GetConstructor);

impl Resolver<'_> {
    pub fn resolve<T: 'static>(&self) -> Option<T> {
        self.0
            .get_constructor(&TypeId::of::<T>())
            .and_then(|constructor| (constructor)(self).downcast::<T>().ok())
            .map(|value| *value)
    }
}

pub trait GetConstructor {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor>;
}

impl GetConstructor for () {
    fn get_constructor(&self, _type_id: &TypeId) -> Option<Constructor> {
        None
    }
}

impl<P: GetConstructor> GetConstructor for Container<P> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.constructors
            .get(type_id)
            .cloned()
            .or(self.parent.get_constructor(type_id))
    }
}

impl<P: GetConstructor> GetConstructor for &Container<P> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.constructors
            .get(type_id)
            .cloned()
            .or(self.parent.get_constructor(type_id))
    }
}

impl<G: GetConstructor> GetConstructor for Arc<G> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.deref().get_constructor(type_id)
    }
}

/// Used to create a value of type `Self` from the `ServiceLocator`.
pub trait Construct {
    fn construct(locator: &Resolver) -> Self;
}
pub trait ConstructAs: Construct {
    type Target;
    fn construct_as(locator: &Resolver) -> Self::Target;
}

macro_rules! impl_delegate_construct {
    ($($type:ty),*) => {
        $(
            impl<T: Construct> Construct for $type {
                fn construct(locator: &Resolver) -> Self {
                    <$type>::new(T::construct(locator))
                }
            }
        )*
    };
}

impl_delegate_construct!(Rc<T>, Arc<T>, RefCell<T>, Mutex<T>);
