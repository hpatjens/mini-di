# Mini-DI

Container for dependency injection.

```rust
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
```