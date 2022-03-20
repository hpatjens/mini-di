use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
};

type Constructor = Rc<dyn for<'r> Fn(&'r mut Container) -> Box<dyn Any>>;

#[derive(Default)]
pub struct Container {
    constructors: BTreeMap<TypeId, Constructor>,
}

impl Container {
    /// Register the `value` for the type `T` to be cloned upon `calling `resolve`.
    pub fn register_clone<T: Clone + 'static>(&mut self, value: T) {
        let value = Box::new(value);
        let constructor = Rc::new(move |_: &mut Container| value.clone() as Box<dyn Any>);
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    /// Register the type `T` to be constructed on every call of `resolve`.
    pub fn register_construct<T: Construct + 'static>(&mut self) {
        let constructor = Rc::new(move |container: &mut Container| {
            Box::new(T::construct(container)) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<T>(), constructor);
    }

    // Register the type `T` to be constructed when it is needed and an `Rc` is given out upon calling `resolve`.
    pub fn register_singleton_as_rc<T: Construct + 'static>(&mut self) {
        let singleton: RefCell<Option<Rc<T>>> = RefCell::new(None);
        let constructor = Rc::new(move |container: &mut Container| {
            if let Some(rc) = &*singleton.borrow() {
                return Box::new(rc.clone()) as Box<dyn Any>;
            }
            let value = Rc::new(T::construct(container));
            *singleton.borrow_mut() = Some(value.clone());
            Box::new(value) as Box<dyn Any>
        });
        self.constructors.insert(TypeId::of::<Rc<T>>(), constructor);
    }

    /// Get a value of the given type.
    pub fn resolve<T: 'static>(&mut self) -> Option<T> {
        let constructor = self.constructors.get(&TypeId::of::<T>()).cloned()?;
        let value = (constructor)(self).downcast::<T>().ok()?;
        Some(*value)
    }
}

/// Used to create aa value of type `Self` from the `Container`.
pub trait Construct {
    fn construct(container: &mut Container) -> Self;
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
        fn construct(_: &mut Container) -> Self {
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
        fn construct(_: &mut Container) -> Self {
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
        fn construct(_container: &mut Container) -> Self {
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
        fn construct(container: &mut Container) -> Self {
            Self {
                audio_manager: container.resolve().unwrap(),
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
        fn construct(container: &mut Container) -> Self {
            Self {
                logger: container.resolve().unwrap(),
            }
        }
    }

    #[test]
    fn test2() {
        let mut container = Container::default();
        container.register_clone::<Rc<dyn AudioManager>>(Rc::new(TestAudioManager));
        container.register_construct::<Player>();
        container.register_singleton_as_rc::<Logger>();
        container.register_singleton_as_rc::<Boss>();
    
        let _audio_manager: Rc<dyn AudioManager> = container.resolve().unwrap();
        let player: Player = container.resolve().unwrap();
        let boss: Rc<Boss> = container.resolve().unwrap();
    
        player.jump();
        boss.hit();
    }
}
