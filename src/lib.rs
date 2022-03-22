use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
};

pub trait FindConstructor {
    fn find_constructor(&self, type_id: &TypeId) -> Option<Constructor>;
}

type Constructor = Rc<dyn for<'r> Fn(&'r mut ServiceLocator) -> Box<dyn Any>>;

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
pub struct ServiceLocator<'parent> {
    parent: Parent<'parent>,
    constructors: BTreeMap<TypeId, Constructor>,
}

impl<'parent> ServiceLocator<'parent> {
    /// Create a new `ServiceLocator` that delegates `resolve` calls to the `parent` when it cannot be satisfied.
    pub fn with_parent(parent: Parent<'parent>) -> Self {
        Self {
            parent,
            constructors: Default::default(),
        }
    }

    /// Register the `value` for the type `T` to be cloned upon `calling `resolve`.
    pub fn register_clone<T: Clone + 'static>(&mut self, value: T) {
        let value = Box::new(value);
        let constructor = Rc::new(move |_: &mut ServiceLocator| value.clone() as Box<dyn Any>);
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    /// Register the type `T` to be constructed on every call of `resolve`.
    pub fn register_construct<T: Construct + 'static>(&mut self) {
        let constructor = Rc::new(move |locator: &mut ServiceLocator| {
            Box::new(T::construct(locator)) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    // Register the type `T` to be constructed when it is needed and an `Rc` is given out upon calling `resolve`.
    pub fn register_singleton_as_rc<T: Construct + 'static>(&mut self) {
        let singleton: RefCell<Option<Rc<T>>> = RefCell::new(None);
        let constructor = Rc::new(move |locator: &mut ServiceLocator| {
            if let Some(rc) = &*singleton.borrow() {
                return Box::new(rc.clone()) as Box<dyn Any>;
            }
            let value = Rc::new(T::construct(locator));
            *singleton.borrow_mut() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<Rc<T>>(), constructor);
    }

    /// Get a value of the given type.
    pub fn resolve<T: 'static>(&mut self) -> Option<T> {
        self.find_constructor(&TypeId::of::<T>())
            .and_then(|constructor| (constructor)(self).downcast::<T>().ok())
            .map(|value| *value)
    }
}

impl<'parent> FindConstructor for ServiceLocator<'parent> {
    fn find_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        match self.constructors.get(type_id).cloned() {
            Some(constructor) => Some(constructor),
            None => match &self.parent {
                Parent::None => None,
                Parent::Borrowed(reference) => reference.find_constructor(type_id),
                Parent::Owned(owned) => owned.find_constructor(type_id),
            },
        }
    }
}

impl<'parent> FindConstructor for &ServiceLocator<'parent> {
    fn find_constructor(&self, type_id: &TypeId) -> Option<Constructor> {
        <ServiceLocator as FindConstructor>::find_constructor(*self, type_id)
    }
}

/// Used to create aa value of type `Self` from the `ServiceLocator`.
pub trait Construct {
    fn construct(locator: &mut ServiceLocator) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AudioManager {
        fn play(&self);
    }

    struct ProductionAudioManager;
    impl AudioManager for ProductionAudioManager {
        fn play(&self) {
            println!("ProductionAudioManager");
        }
    }
    impl Construct for ProductionAudioManager {
        fn construct(_: &mut ServiceLocator) -> Self {
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
        fn construct(_: &mut ServiceLocator) -> Self {
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
        fn construct(_locator: &mut ServiceLocator) -> Self {
            Self
        }
    }

    struct Player {
        audio_manager: Rc<dyn AudioManager>,
    }

    impl Player {
        fn jump(&self) {
            self.audio_manager.play();
        }
    }

    impl Construct for Player {
        fn construct(locator: &mut ServiceLocator) -> Self {
            Self {
                audio_manager: locator.resolve().unwrap(),
            }
        }
    }
    struct Boss {
        logger: Rc<Logger>,
    }

    impl Boss {
        fn hit(&self) {
            self.logger.log("Boss was hit.");
        }
    }

    impl Construct for Boss {
        fn construct(locator: &mut ServiceLocator) -> Self {
            Self {
                logger: locator.resolve().unwrap(),
            }
        }
    }

    #[test]
    fn test2() {
        let mut locator = ServiceLocator::default();
        locator.register_clone::<Rc<dyn AudioManager>>(Rc::new(TestAudioManager));
        locator.register_construct::<Player>();
        locator.register_singleton_as_rc::<Logger>();
        locator.register_singleton_as_rc::<Boss>();

        let _audio_manager: Rc<dyn AudioManager> = locator.resolve().unwrap();
        let player: Player = locator.resolve().unwrap();
        let boss: Rc<Boss> = locator.resolve().unwrap();

        player.jump();
        boss.hit();
    }

    #[test]
    fn test3() {
        let mut parent_locator = ServiceLocator::default();
        parent_locator.register_singleton_as_rc::<Logger>();

        let mut child_locator = ServiceLocator::with_parent(Parent::Borrowed(&parent_locator));
        child_locator.register_construct::<Boss>();

        let boss: Boss = child_locator.resolve().unwrap();
        boss.hit();
    }

    #[test]
    fn test4() {
        let mut child_locator = ServiceLocator::with_parent(Parent::Owned({
            let mut parent_locator = ServiceLocator::default();
            parent_locator.register_singleton_as_rc::<Logger>();
            Box::new(parent_locator)
        }));
        child_locator.register_construct::<Boss>();

        let boss: Boss = child_locator.resolve().unwrap();
        boss.hit();
    }
}
