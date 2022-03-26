use std::{any::{Any, TypeId}, cell::RefCell, collections::BTreeMap, ops::Deref, rc::Rc, sync::{Arc, Mutex}};

pub trait GetConstructor {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor>;
}

type Constructor = Arc<dyn for<'r> Fn(&'r Resolver<'r>) -> Box<dyn Any> + Send + Sync> ;

pub struct Resolver<'r>(&'r dyn GetConstructor);

impl<'r> Resolver<'r> {
    pub fn resolve<T: 'static>(&self) -> Option<T> {
        self.0
            .get_constructor(&TypeId::of::<T>())
            .and_then(|constructor| (constructor)(self).downcast::<T>().ok())
            .map(|value| *value)
    }
}

pub trait Register {
    /// Register the `value` for the type `T` to be cloned upon `calling `resolve`.
    fn register_clone<T: Clone + Send + Sync + 'static>(&mut self, value: T);

    /// Register the type `T` to be constructed on every call of `resolve`.
    fn register_construct<T: Construct + 'static>(&mut self);

    // Register the type `T` to be constructed when it is needed and an `Rc` is given out upon calling `resolve`.
    fn register_singleton_as_rc<T: Construct + Send + Sync + 'static>(&mut self);
}

impl GetConstructor for () {
    fn get_constructor(&self, _type_id: &TypeId) -> Option<Constructor> {
        None
    }
}

impl<G: GetConstructor> GetConstructor for Arc<G> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.deref().get_constructor(type_id)
    }
}

pub struct Container<P: GetConstructor = ()> {
    parent: P,
    constructors: BTreeMap<TypeId, Constructor>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            parent: (),
            constructors: Default::default(),
        }
    }

    pub fn with_parent<P: GetConstructor>(parent: P) -> Container<P> {
        Container {
            parent,
            constructors: Default::default(),
        }
    }
}

impl<P: GetConstructor> Register for Container<P> {
    fn register_clone<T: Clone + Send + Sync + 'static>(&mut self, value: T) {
        let value = Box::new(value);
        let constructor = Arc::new(move |_: &Resolver| value.clone() as Box<dyn Any>);
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    fn register_construct<T: Construct + 'static>(&mut self) {
        let constructor = Arc::new(move |locator: &Resolver| {
            Box::new(T::construct(locator)) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    fn register_singleton_as_rc<T: Construct + Send + Sync + 'static>(&mut self) {
        let singleton: Mutex<Option<Arc<T>>> = Mutex::new(None);
        let constructor = Arc::new(move |locator: &Resolver| {
            if let Some(rc) = &*singleton.lock().unwrap() {
                return Box::new(rc.clone()) as Box<dyn Any>;
            }
            let value = Arc::new(T::construct(locator));
            *singleton.lock().unwrap() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<Arc<T>>(), constructor);
    }
}

impl<P: GetConstructor> GetConstructor for Container<P> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.constructors.get(type_id).cloned().or(self.parent.get_constructor(type_id))
    }
}

impl<P: GetConstructor> GetConstructor for &Container<P> {
    fn get_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        self.constructors.get(type_id).cloned().or(self.parent.get_constructor(type_id))
    }
}

/// Used to create aa value of type `Self` from the `ServiceLocator`.
pub trait Construct {
    fn construct(locator: &Resolver) -> Self;
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
    fn test2() {
        let mut locator = Container::new();
        locator.register_clone::<Arc<dyn AudioManager>>(Arc::new(TestAudioManager));
        locator.register_construct::<Player>();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_singleton_as_rc::<Boss>();

        let resolver = Resolver(&locator);
        
        let _audio_manager: Arc<dyn AudioManager> = resolver.resolve().unwrap();
        let player: Player = resolver.resolve().unwrap();
        let boss: Arc<Boss> = resolver.resolve().unwrap();

        player.jump();
        boss.hit();
    }

    #[test]
    fn test3() {
        let mut parent_locator = Container::new();
        parent_locator.register_singleton_as_rc::<Logger>();

        let mut child_locator = Container::with_parent(&parent_locator);
        child_locator.register_construct::<Boss>();

        let child_resolver = Resolver(&child_locator);

        let boss: Boss = child_resolver.resolve().unwrap();
        boss.hit();
    }

    #[test]
    fn test4() {
        let mut child_locator = Container::with_parent({
            let mut parent_locator = Container::new();
            parent_locator.register_singleton_as_rc::<Logger>();
            Arc::new(parent_locator)
        });
        child_locator.register_construct::<Boss>();

        let child_resolver = Resolver(&child_locator);

        let boss: Boss = child_resolver.resolve().unwrap();
        boss.hit();
    }

    #[test]
    fn test5() {
        let mut locator = Container::new();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_construct::<Rc<RefCell<Boss>>>();

        let resolver = Resolver(&locator);

        let boss: Rc<RefCell<Boss>> = resolver.resolve().unwrap();
        boss.borrow_mut().fire();
    }

    #[test]
    fn test6() {
        let mut locator = Container::new();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_construct::<Arc<Mutex<Boss>>>();

        let resolver = Resolver(&locator);

        let boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
        boss.lock().unwrap().fire();
    }

    #[test]
    fn test7() {
        let mut locator = Container::new();
        locator.register_construct::<Arc<Mutex<Boss>>>();
        let locator = locator;

        let locator = Arc::new(Mutex::new(locator));

        std::thread::spawn(move || {
            let locator = locator.lock().unwrap();
            let resolver = Resolver(&*locator);
            let _boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
        });
    }
}
