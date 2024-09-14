use std::{mem, sync::LazyLock};

use bevy::{
    ecs::{entity::MapEntities, reflect::ReflectMapEntities},
    prelude::*,
    tasks::IoTaskPool,
};

use bevy_playground::{Action, History, HistoryItem, PerformAction, Redo, Undo};

const SCENE_FILE: &str = "scene.scn";

// serialize these components
const COMPONENT_FILTER: LazyLock<SceneFilter> = LazyLock::new(|| {
    SceneFilter::deny_all()
        .allow::<SetLevel>()
        .allow::<MoveEntity>()
        .allow::<Player>()
        .allow::<Transform>()
        .allow::<GlobalTransform>()
        .allow::<Sprite>()
        .allow::<Handle<Image>>()
        .allow::<Visibility>()
        .allow::<InheritedVisibility>()
        .allow::<ViewVisibility>()
});

// and these resources
const RESOURCE_FILTER: LazyLock<SceneFilter> =
    LazyLock::new(|| SceneFilter::deny_all().allow::<History>().allow::<Level>());

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_resource::<Level>()
        .init_resource::<History>()
        .register_type::<(History, SetLevel, MoveEntity, Level, Player)>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (movement_input, level_input, history_input, save_load_input),
        )
        .run();
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
struct Player;

#[derive(Resource, Reflect, Default, Debug)]
#[reflect(Resource, Default)]
struct Level(u32);

fn setup(mut commands: Commands) {
    println!(
        "
wasd: move player
1..=9: add level
left arrow: undo action
right arrow: redo action
i: save scene
o: load scene
"
    );

    commands.spawn(Camera2dBundle::default());
    commands.spawn((
        Player,
        SpriteBundle {
            transform: Transform {
                scale: Vec3::splat(100.0),
                ..Default::default()
            },
            ..Default::default()
        },
    ));
}

#[derive(Component, Reflect, Clone)]
#[reflect(Component)]
#[require(HistoryItem(HistoryItem::new::<Self>))]
struct SetLevel(u32);

impl Action for SetLevel {
    fn apply(&mut self, world: &mut World) {
        let mut level = world.resource_mut::<Level>();
        mem::swap(&mut self.0, &mut level.0);
    }

    fn undo(&mut self, world: &mut World) {
        Action::apply(self, world);
    }
}

#[derive(Component, Reflect, Clone)]
#[reflect(Component, MapEntities)]
#[require(HistoryItem(HistoryItem::new::<Self>))]
struct MoveEntity {
    entity: Entity,
    delta: Vec3,
}

impl Action for MoveEntity {
    fn apply(&mut self, world: &mut World) {
        let mut transform = world.get_mut::<Transform>(self.entity).unwrap();
        transform.translation += self.delta;
    }

    fn undo(&mut self, world: &mut World) {
        let mut transform = world.get_mut::<Transform>(self.entity).unwrap();
        transform.translation -= self.delta;
    }
}

impl MapEntities for MoveEntity {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.entity = mapper.map_entity(self.entity);
    }
}

fn movement_input(
    mut commands: Commands,
    key: Res<ButtonInput<KeyCode>>,
    player: Query<Entity, With<Player>>,
) {
    let just_pressed_axis = |low, high| match (key.just_pressed(low), key.just_pressed(high)) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        (false, false) | (true, true) => 0.0,
    };

    let dir = Vec2 {
        x: just_pressed_axis(KeyCode::KeyA, KeyCode::KeyD),
        y: just_pressed_axis(KeyCode::KeyS, KeyCode::KeyW),
    };

    if dir != Vec2::ZERO {
        commands.add(PerformAction {
            action: MoveEntity {
                entity: player.single(),
                delta: (dir * 100.0).extend(0.0),
            },
        });
    }
}

fn level_input(mut commands: Commands, key: Res<ButtonInput<KeyCode>>) {
    for key in key.get_just_pressed() {
        if let Some(digit) = match *key {
            KeyCode::Digit1 => Some(1),
            KeyCode::Digit2 => Some(2),
            KeyCode::Digit3 => Some(3),
            KeyCode::Digit4 => Some(4),
            KeyCode::Digit5 => Some(5),
            KeyCode::Digit6 => Some(6),
            KeyCode::Digit7 => Some(7),
            KeyCode::Digit8 => Some(8),
            KeyCode::Digit9 => Some(9),
            _ => None,
        } {
            commands.add(move |world: &mut World| {
                let new_level = world.resource::<Level>().0 + digit;
                Command::apply(
                    PerformAction {
                        action: SetLevel(new_level),
                    },
                    world,
                );
            });
        }
        commands.add(debug_history);
    }
}

fn history_input(mut commands: Commands, key: Res<ButtonInput<KeyCode>>) {
    if key.just_pressed(KeyCode::ArrowLeft) {
        commands.add(Undo);
    }

    if key.just_pressed(KeyCode::ArrowRight) {
        commands.add(Redo);
    }
}

fn save_load_input(
    mut commands: Commands,
    key: Res<ButtonInput<KeyCode>>,
    query: Query<Entity, (Without<Window>, Without<Camera>)>,
    asset_server: Res<AssetServer>,
) {
    if key.just_pressed(KeyCode::KeyI) {
        commands.add(save_scene);
    }

    if key.just_pressed(KeyCode::KeyO) {
        commands.remove_resource::<History>();
        commands.remove_resource::<Level>();
        for entity in &query {
            commands.entity(entity).despawn_recursive();
        }
        commands.spawn(DynamicSceneBundle {
            scene: asset_server.load(SCENE_FILE),
            ..Default::default()
        });
    }
}

fn debug_history(world: &mut World) {
    let Some((history, level)) = world
        .get_resource::<History>()
        .zip(world.get_resource::<Level>())
    else {
        return;
    };

    print!("[ ");
    for e in history.past.iter() {
        match world.get::<SetLevel>(*e) {
            Some(level) => print!("{} ", level.0),
            None => print!("* "),
        }
    }
    print!("[{}]", level.0);
    if !history.future.is_empty() {
        print!(" ");
    }
    for (i, e) in history.future.iter().rev().enumerate() {
        match world.get::<SetLevel>(*e) {
            Some(level) => print!("{}", level.0),
            None => print!("*"),
        }
        if i != history.future.len() - 1 {
            print!(" ");
        }
    }
    println!(" ]");
}

fn save_scene(world: &mut World) {
    use std::{fs::File, io::Write};

    let mut query = world.query_filtered::<Entity, Without<Camera>>();

    let scene = DynamicSceneBuilder::from_world(world)
        .with_filter((*COMPONENT_FILTER).clone())
        .with_resource_filter((*RESOURCE_FILTER).clone())
        .extract_resources()
        .extract_entities(query.iter(world))
        .remove_empty_entities()
        .build();

    // Scenes can be serialized like this:
    let type_registry = world.resource::<AppTypeRegistry>();
    let serialized_scene = scene.serialize(&type_registry.read()).unwrap();

    // Showing the scene in the console
    info!("{}", serialized_scene);

    // Writing the scene to a new file. Using a task to avoid calling the filesystem APIs in a system
    // as they are blocking
    // This can't work in Wasm as there is no filesystem access
    #[cfg(not(target_arch = "wasm32"))]
    IoTaskPool::get()
        .spawn(async move {
            // Write the scene RON data to file
            File::create(format!("./assets/{SCENE_FILE}"))
                .and_then(|mut file| file.write(serialized_scene.as_bytes()))
                .expect("Error while writing scene to file");
        })
        .detach();
}
