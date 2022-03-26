use std::{any::{Any, TypeId}, cell::RefCell, collections::BTreeMap, rc::Rc, sync::{Arc, Mutex}};

pub trait FindConstructor {
    fn find_constructor(&self, type_id: &TypeId) -> Option<Constructor>;
}

type Constructor = Arc<dyn for<'r> Fn(&'r ServiceLocator) -> Box<dyn Any> + Send + Sync> ;

pub enum Parent<'parent> {
    None,
    Borrowed(&'parent dyn FindConstructor),
    Owned(Box<dyn FindConstructor>),
}

impl<'parent> Default for Parent<'parent> {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Default)]
pub struct ServiceLocator {
    constructors: BTreeMap<TypeId, Constructor>,
}

impl ServiceLocator {
    /// Register the `value` for the type `T` to be cloned upon `calling `resolve`.
    pub fn register_clone<T: Clone + Send + Sync + 'static>(&mut self, value: T) {
        let value = Box::new(value);
        let constructor = Arc::new(move |_: &ServiceLocator| value.clone() as Box<dyn Any>);
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    /// Register the type `T` to be constructed on every call of `resolve`.
    pub fn register_construct<T: Construct + 'static>(&mut self) {
        let constructor = Arc::new(move |locator: &ServiceLocator| {
            Box::new(T::construct(locator)) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    // Register the type `T` to be constructed when it is needed and an `Rc` is given out upon calling `resolve`.
    pub fn register_singleton_as_rc<T: Construct + Send + Sync + 'static>(&mut self) {
        let singleton: Mutex<Option<Arc<T>>> = Mutex::new(None);
        let constructor = Arc::new(move |locator: &ServiceLocator| {
            if let Some(rc) = &*singleton.lock().unwrap() {
                return Box::new(rc.clone()) as Box<dyn Any>;
            }
            let value = Arc::new(T::construct(locator));
            *singleton.lock().unwrap() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<Arc<T>>(), constructor);
    }

    /// Get a value of the given type.
    pub fn resolve<T: 'static>(&self) -> Option<T> {
        self.constructors.get(&TypeId::of::<T>()).cloned()
            .and_then(|constructor| (constructor)(self).downcast::<T>().ok())
            .map(|value| *value)
    }
}

/// Used to create aa value of type `Self` from the `ServiceLocator`.
pub trait Construct {
    fn construct(locator: &ServiceLocator) -> Self;
}

macro_rules! impl_delegate_construct {
    ($($type:ty),*) => {
        $(
            impl<T: Construct> Construct for $type {
                fn construct(locator: &ServiceLocator) -> Self {
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
        fn construct(_: &ServiceLocator) -> Self {
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
        fn construct(_: &ServiceLocator) -> Self {
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
        fn construct(_locator: &ServiceLocator) -> Self {
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
        fn construct(locator: &ServiceLocator) -> Self {
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
        fn construct(locator: &ServiceLocator) -> Self {
            Self {
                logger: locator.resolve().unwrap(),
            }
        }
    }

    #[test]
    fn test2() {
        let mut locator = ServiceLocator::default();
        locator.register_clone::<Arc<dyn AudioManager>>(Arc::new(TestAudioManager));
        locator.register_construct::<Player>();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_singleton_as_rc::<Boss>();

        let _audio_manager: Arc<dyn AudioManager> = locator.resolve().unwrap();
        let player: Player = locator.resolve().unwrap();
        let boss: Arc<Boss> = locator.resolve().unwrap();

        player.jump();
        boss.hit();
    }

    #[test]
    fn test5() {
        let mut locator = ServiceLocator::default();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_construct::<Rc<RefCell<Boss>>>();

        let boss: Rc<RefCell<Boss>> = locator.resolve().unwrap();
        boss.borrow_mut().fire();
    }

    #[test]
    fn test6() {
        let mut locator = ServiceLocator::default();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_construct::<Arc<Mutex<Boss>>>();
        let locator = Arc::new(locator);

        let boss: Arc<Mutex<Boss>> = locator.resolve().unwrap();
        boss.lock().unwrap().fire();
    }

    #[test]
    fn test7() {
        let mut locator = ServiceLocator::default();
        locator.register_construct::<Arc<Mutex<Boss>>>();
        let locator = locator;

        let locator = Arc::new(Mutex::new(locator));

        std::thread::spawn(move || {
            let boss: Arc<Mutex<Boss>> = locator.lock().unwrap().resolve().unwrap();
        });
    }
}
