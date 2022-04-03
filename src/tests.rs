use crate::*;

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
    let _value: Rc<TestAudioManager> = container.as_resolver().resolve().unwrap();
}

#[test]
fn when_construct_with() {
    let mut container = Container::new();
    container
        .when::<Rc<dyn AudioManager>>()
        .construct_with(|resolver| <Rc<TestAudioManager> as Construct>::construct(resolver))
        .unwrap();
    let _value: Rc<dyn AudioManager> = container.as_resolver().resolve().unwrap();
}

#[test]
fn when_construct() {
    let mut container = Container::new();
    container
        .when::<Rc<dyn AudioManager>>()
        .construct::<Rc<TestAudioManager>>()
        .unwrap();
    let _value: Rc<dyn AudioManager> = container.as_resolver().resolve().unwrap();
}

#[test]
fn test2() {
    let mut locator = Container::new();
    locator
        .when::<Arc<dyn AudioManager>>()
        .clone(Arc::new(TestAudioManager))
        .unwrap();
    locator.when::<Player>().construct_it().unwrap();
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
    child_locator.when::<Boss>().construct_it().unwrap();

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
    child_locator.when::<Boss>().construct_it().unwrap();

    let child_resolver = child_locator.as_resolver();

    let boss: Boss = child_resolver.resolve().unwrap();
    boss.hit();
}

#[test]
fn test5() {
    let mut locator = Container::new();
    locator.register_singleton::<Logger>().unwrap();
    locator.when::<Rc<RefCell<Boss>>>().construct_it().unwrap();

    let resolver = locator.as_resolver();

    let boss: Rc<RefCell<Boss>> = resolver.resolve().unwrap();
    boss.borrow_mut().fire();
}

#[test]
fn test6() {
    let mut locator = Container::new();
    locator.register_singleton::<Logger>().unwrap();
    locator.when::<Arc<Mutex<Boss>>>().construct_it().unwrap();

    let resolver = locator.as_resolver();

    let boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
    boss.lock().unwrap().fire();
}

#[test]
fn test7() {
    let mut locator = Container::new();
    locator.when::<Arc<Mutex<Boss>>>().construct_it().unwrap();
    let locator = locator;

    let locator = Arc::new(Mutex::new(locator));

    std::thread::spawn(move || {
        let locator = locator.lock().unwrap();
        let resolver = locator.as_resolver();
        let _boss: Arc<Mutex<Boss>> = resolver.resolve().unwrap();
    });
}
