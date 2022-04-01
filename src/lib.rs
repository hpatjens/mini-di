use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex},
};

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
    fn clone(self, value: R) -> Result<()> {
        self.container.register_clone(value)
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static + Construct,
    P: GetConstructor,
{
    fn construct_it(self) -> Result<()> {
        self.container.register_construct::<R>()
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static,
    P: GetConstructor,
{
    fn construct<E>(self) -> Result<()>
    where
        E: 'static + ConstructAs<Target=R>,
    {
        self.container.register_construct_other::<R, E>()
    }
}

impl<'container, R, P> Registration<'container, R, P>
where
    R: 'static,
    P: GetConstructor,
{
    fn construct_with<F>(self, constructor: F) -> Result<()>
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

    /// Register the `value` for the type `T` to be cloned upon calling `resolve`.
    pub fn register_clone<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Result<()> {
        let value = Box::new(value);
        let constructor = Arc::new(move |_: &Resolver| value.clone() as Box<dyn Any>);
        self.register_constructor::<T>(constructor)
    }

    /// Register the type `T` to be constructed on every call of `resolve`.
    pub fn register_construct<T: Construct + 'static>(&mut self) -> Result<()> {
        let constructor =
            Arc::new(move |locator: &Resolver| Box::new(T::construct(locator)) as Box<dyn Any>);
        self.register_constructor::<T>(constructor)
    }

    pub fn register_construct_other<T: 'static, E: ConstructAs<Target=T> + 'static>(
        &mut self,
    ) -> Result<()> {
        let constructor = Arc::new(move |locator: &Resolver| {
            let new = Box::new(E::construct_as(locator));
            new as Box<dyn Any>
        });
        self.register_constructor::<T>(constructor)
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

    pub fn resolve_for<T: 'static, E: 'static>(&self) -> Option<E> {
        self.0
            .get_constructor(&TypeId::of::<T>())
            .and_then(|constructor| (constructor)(self).downcast::<E>().ok())
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

/// Used to create aa value of type `Self` from the `ServiceLocator`.
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

#[cfg(test)]
mod tests {
    use super::*;

    trait AudioManager: Send + Sync {
        fn play(&self);
    }

    struct ProductionAudioManager;
    impl AudioManager for ProductionAudioManager {
        fn play(&self) {
            println!("ProductionAudioManager");
        }
    }
    impl Construct for ProductionAudioManager {
        fn construct(_: &Resolver) -> Self {
            Self
        }
    }
    impl ConstructAs for Rc<TestAudioManager> {
        type Target = Rc<dyn AudioManager>;
        fn construct_as(locator: &Resolver) -> Self::Target {
            <Rc<TestAudioManager> as Construct>::construct(locator)
        }
    }

    struct TestAudioManager;
    impl AudioManager for TestAudioManager {
        fn play(&self) {
            println!("TestAudioManager");
        }
    }
    impl Construct for TestAudioManager {
        fn construct(_: &Resolver) -> Self {
            Self
        }
    }

    struct Logger;
    impl Logger {
        fn log(&self, message: &str) {
            println!("{message}");
        }
    }
    impl Construct for Logger {
        fn construct(_locator: &Resolver) -> Self {
            Self
        }
    }

    struct Player {
        audio_manager: Arc<dyn AudioManager>,
    }

    impl Player {
        fn jump(&self) {
            self.audio_manager.play();
        }
    }

    impl Construct for Player {
        fn construct(locator: &Resolver) -> Self {
            Self {
                audio_manager: locator.resolve().unwrap(),
            }
        }
    }
    struct Boss {
        logger: Arc<Logger>,
    }

    impl Boss {
        fn hit(&self) {
            self.logger.log("Boss was hit.");
        }

        fn fire(&mut self) {
            self.logger.log("Boss fired.");
        }
    }

    impl Construct for Boss {
        fn construct(locator: &Resolver) -> Self {
            Self {
                logger: locator.resolve().unwrap(),
            }
        }
    }

    #[test]
    fn when_clone() {
        let mut container = Container::new();
        container.when::<u32>().clone(42).unwrap();
        let value: u32 = container.as_resolver().resolve().unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn when_construct_it() {
        let mut container = Container::new();
        container
            .when::<Rc<TestAudioManager>>()
            .construct_it()
            .unwrap();
        let value: Rc<TestAudioManager> = container.as_resolver().resolve().unwrap();
    }

    #[test]
    fn when_construct_with() {
        let mut container = Container::new();
        container
            .when::<Rc<dyn AudioManager>>()
            .construct_with(|resolver| <Rc<TestAudioManager> as Construct>::construct(resolver))
            .unwrap();
        let value: Rc<dyn AudioManager> = container.as_resolver().resolve().unwrap();
    }

    #[test]
    fn when_construct() {
        let mut container = Container::new();
        container
            .when::<Rc<dyn AudioManager>>()
            .construct::<Rc<TestAudioManager>>()
            .unwrap();
        let value: Rc<dyn AudioManager> = container
            .as_resolver()
            .resolve()
            .unwrap();
    }

    #[test]
    fn test2() {
        let mut locator = Container::new();
        locator
            .register_clone::<Arc<dyn AudioManager>>(Arc::new(TestAudioManager))
            .unwrap();
        locator.register_construct::<Player>().unwrap();
        locator.register_singleton::<Logger>().unwrap();
        locator.register_singleton::<Boss>().unwrap();

        let resolver = locator.as_resolver();

        let _audio_manager: Arc<dyn AudioManager> = resolver.resolve().unwrap();
        let player: Player = resolver.resolve().unwrap();
        let boss: Arc<Boss> = resolver.resolve().unwrap();

        player.jump();
        boss.hit();
    }

    #[test]
    fn test3() {
        let mut parent_locator = Container::new();
        parent_locator.register_singleton::<Logger>().unwrap();

        let mut child_locator = Container::with_parent(&parent_locator);
        child_locator.register_construct::<Boss>().unwrap();

        let child_resolver = child_locator.as_resolver();

        let boss: Boss = child_resolver.resolve().unwrap();
        boss.hit();
    }

    #[test]
    fn test4() {
        let mut child_locator = Container::with_parent({
            let mut parent_locator = Container::new();
            parent_locator.register_singleton::<Logger>().unwrap();
            Arc::new(parent_locator)
        });
        child_locator.register_construct::<Boss>().unwrap();

        let child_resolver = child_locator.as_resolver();

        let boss: Boss = child_resolver.resolve().unwrap();
        boss.hit();
    }

    #[test]
    fn test5() {
        let mut locator = Container::new();
        locator.register_singleton::<Logger>().unwrap();
        locator.register_construct::<Rc<RefCell<Boss>>>().unwrap();

        let resolver = locator.as_resolver();

        let boss: Rc<RefCell<Boss>> = resolver.resolve().unwrap();
        boss.borrow_mut().fire();
    }

    #[test]
    fn test6() {
        let mut locator = Container::new();
        locator.register_singleton::<Logger>().unwrap();
        locator.register_construct::<Arc<Mutex<Boss>>>().unwrap();

        let resolver = locator.as_resolver();

        let boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
        boss.lock().unwrap().fire();
    }

    #[test]
    fn test7() {
        let mut locator = Container::new();
        locator.register_construct::<Arc<Mutex<Boss>>>().unwrap();
        let locator = locator;

        let locator = Arc::new(Mutex::new(locator));

        std::thread::spawn(move || {
            let locator = locator.lock().unwrap();
            let resolver = locator.as_resolver();
            let _boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
        });
    }

    #[test]
    fn my_test() {
        let mut container = Container::new();

        // construct
        container
            .when::<Rc<dyn AudioManager>>()
            .construct::<Rc<TestAudioManager>>();
        container.when::<Boss>().construct_it();
    }
}
