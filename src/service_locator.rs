use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Function signature used for creating the dependencies when they are requested
type Constructor = Arc<dyn for<'r> Fn(&'r Locator) -> Box<dyn Any> + Send + Sync>;

/// Container storing the dependencies
///
/// You can use the [`when`](Container::when) method to register dependencies are request them by calling the [`locate`](Locator::locate) method.
///
/// # Examples
///
/// ```
/// use mini_di::service_locator::Container;
/// let mut container = Container::new();
/// container.when::<u32>().clone(42).unwrap();
/// let value: u32 = container.as_locator().locate().unwrap();
/// assert_eq!(value, 42);
/// ```
pub struct Container<P: GetConstructor = ()> {
    parent: P,
    constructors: BTreeMap<TypeId, Constructor>,
}

impl Container {
    /// Create a new `Container` with no parent container
    ///
    /// # Examples
    ///
    /// ```
    /// use mini_di::service_locator::Container;
    /// let container = Container::new();
    /// ```
    pub fn new() -> Self {
        Self {
            parent: (),
            constructors: Default::default(),
        }
    }

    /// Create a new `Container` with the given `Container` as fallback
    ///
    /// # Examples
    ///
    /// ```
    /// use mini_di::service_locator::Container;
    /// let mut container1 = Container::new();
    /// container1.when::<u32>().clone(42);
    /// let mut container2 = Container::with_parent(&container1);
    /// let value: u32 = container2.as_locator().locate().unwrap();
    /// assert_eq!(value, 42);
    /// ```
    pub fn with_parent<P: GetConstructor>(parent: P) -> Container<P> {
        Container {
            parent,
            constructors: Default::default(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// Dependency was already registered
    AlreadyRegistered,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Builder for creating new dependencies on a `Container`
pub struct Registration<'container, R, P: GetConstructor> {
    _phantom: PhantomData<R>,
    container: &'container mut Container<P>,
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: Sync + Send + 'static + Clone,
    P: GetConstructor,
{
    /// Returns a new clone of the given `value` every time the dependency is requested by [`locate`](Locator::locate)
    ///
    /// # Examples
    ///
    /// ```
    /// use mini_di::service_locator::Container;
    /// let mut container = Container::new();
    /// container.when::<u32>().clone(42).unwrap();
    /// let value: u32 = container.as_locator().locate().unwrap();
    /// assert_eq!(value, 42);
    /// ```
    pub fn clone(self, value: R) -> Result<()> {
        let value = Box::new(value);
        let constructor = Arc::new(move |_: &Locator| value.clone() as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static + Construct,
    P: GetConstructor,
{
    /// Constructs the requested dependency every time it is requested by [`locate`](Locator::locate)
    ///
    /// # Examples
    ///
    /// ```
    /// use mini_di::service_locator::{Container, Locator, Construct};
    ///
    /// #[derive(Debug, Eq, PartialEq)]
    /// struct MyValue;
    ///
    /// impl Construct for MyValue {
    ///     fn construct(locator: &Locator) -> Self {
    ///         Self
    ///     }
    /// }
    ///
    /// let mut container = Container::new();
    /// container.when::<MyValue>().construct_it().unwrap();
    /// let value: MyValue = container.as_locator().locate().unwrap();
    /// assert_eq!(value, MyValue);
    /// ```
    pub fn construct_it(self) -> Result<()> {
        let constructor =
            Arc::new(move |locator: &Locator| Box::new(R::construct(locator)) as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static,
    P: GetConstructor,
{
    /// Constructs the requested dependency by constructing a different type every time it is requested by [`locate`](Locator::locate)
    ///
    /// # Examples
    ///
    /// ```
    /// use mini_di::service_locator::{Container, Locator, ConstructAs};
    ///
    /// trait Value {
    ///     fn get(&self) -> u32;
    /// }
    ///
    /// #[derive(Debug, Eq, PartialEq)]
    /// struct MyValue;
    ///
    /// impl Value for MyValue {
    ///     fn get(&self) -> u32 { 42 }
    /// }
    /// 
    /// impl ConstructAs for Box<MyValue> {
    ///     type Target = Box<dyn Value>;
    ///     fn construct_as(locator: &Locator) -> Self::Target {
    ///         Box::new(MyValue)
    ///     }
    /// }
    ///
    /// let mut container = Container::new();
    /// container.when::<Box<dyn Value>>().construct::<Box<MyValue>>().unwrap();
    /// let value: Box<dyn Value> = container.as_locator().locate().unwrap();
    /// assert_eq!(value.get(), 42);
    /// ```
    pub fn construct<E>(self) -> Result<()>
    where
        E: 'static + ConstructAs<Target = R>,
    {
        let constructor = Arc::new(move |locator: &Locator| {
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
    /// Constructs the requested dependency by calling the given function `constructor`
    /// 
    /// # Examples
    /// 
    /// ```
    /// use mini_di::service_locator::{Container, Locator, ConstructAs};
    /// let mut container = Container::new();
    /// container.when::<u32>().construct_with(|_locator| 42).unwrap();
    /// let value: u32 = container.as_locator().locate().unwrap();
    /// assert_eq!(value, 42);
    /// ```
    pub fn construct_with<F>(self, constructor: F) -> Result<()>
    where
        F: Fn(&Locator) -> R + Send + Sync + 'static,
    {
        let constructor =
            Arc::new(move |resolver: &Locator| Box::new((constructor)(resolver)) as Box<dyn Any>);
        self.container.register_constructor::<R>(constructor)
    }
}

impl<'container, R, P> Registration<'container, Arc<R>, P>
where
    R: 'static + ?Sized,
    P: GetConstructor,
{
    /// Returns a builder for singletons
    /// 
    /// # Examples
    /// 
    /// ```
    /// use std::sync::Arc;
    /// use mini_di::service_locator::Container;
    /// let mut container = Container::new();
    /// container
    ///     .when::<Arc<u32>>()
    ///     .singleton()
    ///     .construct_with(|_locator| Arc::new(42))
    ///     .unwrap();
    /// let value: Arc<u32> = container.as_locator().locate().unwrap();
    /// assert_eq!(*value, 42);
    /// ```
    pub fn singleton(self) -> Singleton<'container, Arc<R>, P> {
        Singleton {
            container: self.container,
            phantom: self._phantom,
        }
    }
}

pub struct Singleton<'container, R, P>
where
    P: GetConstructor,
{
    container: &'container mut Container<P>,
    phantom: PhantomData<R>,
}

impl<'container, R, P> Singleton<'container, Arc<R>, P>
where
    R: 'static + Construct + Send + Sync,
    P: GetConstructor,
{
    pub fn construct_it(self) -> Result<()> {
        self.construct_with(|locator| Arc::new(R::construct(locator)))
    }
}

impl<'container, R, P> Singleton<'container, Arc<R>, P>
where
    R: 'static + Send + Sync + ?Sized,
    P: GetConstructor,
{
    pub fn construct<E>(self) -> Result<()>
    where
        E: 'static + ConstructAs<Target = Arc<R>> + Send + Sync,
    {
        self.construct_with(|locator| E::construct_as(locator))
    }
}

impl<'container, R, P> Singleton<'container, Arc<R>, P>
where
    R: 'static + Send + Sync + ?Sized,
    P: GetConstructor,
{
    pub fn construct_with<F>(self, constructor: F) -> Result<()>
    where
        F: Fn(&Locator) -> Arc<R> + Send + Sync + 'static,
    {
        let singleton: Mutex<Option<Arc<R>>> = Mutex::new(None);
        let constructor = Arc::new(move |locator: &Locator| {
            if let Some(arc) = &*singleton.lock().unwrap() {
                return Box::new(arc.clone()) as Box<dyn Any>;
            }
            let value = constructor(locator);
            *singleton.lock().unwrap() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.container.register_constructor::<Arc<R>>(constructor)
    }
}

impl<P: GetConstructor> Container<P> {
    /// Creates a builder for registering a new dependency
    /// 
    /// ```
    /// use mini_di::service_locator::Container;
    /// let mut container = Container::new();
    /// container.when::<u32>().clone(42).unwrap();
    /// let value: u32 = container.as_locator().locate().unwrap();
    /// assert_eq!(value, 42);
    /// ```
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

    /// Get a `Locator` that borrows the `Container`
    pub fn as_locator(&self) -> Locator {
        Locator(self)
    }
}

pub struct Locator<'r>(&'r dyn GetConstructor);

impl Locator<'_> {
    pub fn locate<T: 'static>(&self) -> Option<T> {
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
    fn construct(locator: &Locator) -> Self;
}
pub trait ConstructAs {
    type Target;
    fn construct_as(locator: &Locator) -> Self::Target;
}

macro_rules! impl_delegate_construct {
($($type:ty),*) => {
    $(
        impl<T: Construct> Construct for $type {
            fn construct(locator: &Locator) -> Self {
                <$type>::new(T::construct(locator))
            }
        }
    )*
};
}

impl_delegate_construct!(Rc<T>, Arc<T>, RefCell<T>, Mutex<T>);
