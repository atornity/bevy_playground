use std::collections::VecDeque;

use bevy::{
    ecs::{entity::MapEntities, reflect::ReflectMapEntitiesResource},
    prelude::*,
    reflect::FromType,
};

pub trait Action: Component + Placeholder {
    fn apply(&mut self, world: &mut World);
    fn undo(&mut self, world: &mut World);
}

/// Types that need a [`Default`] like value when there's no sensible default representation of said type.
pub trait Placeholder {
    fn placeholder() -> Self;
}

impl<T: Default> Placeholder for T {
    fn placeholder() -> Self {
        T::default()
    }
}

/// Run an action, pushing it to the undo stack.
pub struct PerformAction<T: Action> {
    pub action: T,
}

impl<T: Action> Command for PerformAction<T> {
    fn apply(mut self, world: &mut World) {
        // since HistoryItem is not Reflect it must be added as a required components to be deserializable
        #[cfg(debug_assertions)]
        {
            let action_id = world.init_component::<T>();
            let history_item_id = world.init_component::<HistoryItem>();
            let info = world.components().get_info(action_id).unwrap();
            if !info
                .required_components()
                .iter_ids()
                .any(|id| id == history_item_id)
            {
                warn!(
                    "Deserialization won't work unless `{0}` requires `{1}`, try annotating `{0}` with `#[require({1}::new::<Self>)]`",
                    std::any::type_name::<T>(),
                    std::any::type_name::<HistoryItem>()
                );
            }
        }

        self.action.apply(world);
        let entity = world.spawn((self.action, HistoryItem::new::<T>())).id();
        let future = world.resource_mut::<History>().push(entity);
        for entity in future {
            world.despawn(entity);
        }
    }
}

/// Undo the last action
pub struct Undo;

impl Command for Undo {
    fn apply(self, world: &mut World) {
        if let Some(entity) = world.resource_mut::<History>().back() {
            let item = *world.get::<HistoryItem>(entity).unwrap();
            item.undo(world, entity);
        } else {
            info!("end of history");
        }
    }
}

/// Redo the last action
pub struct Redo;

impl Command for Redo {
    fn apply(self, world: &mut World) {
        if let Some(entity) = world.resource_mut::<History>().forward() {
            let item = *world.get::<HistoryItem>(entity).unwrap();
            item.redo(world, entity);
        } else {
            info!("end of history");
        }
    }
}

#[derive(Resource, Reflect, Default, Debug, Clone)]
#[reflect(Resource, MapEntitiesResource, Default)]
pub struct History {
    pub items: VecDeque<Entity>,
    pub index: usize,
}

impl MapEntities for History {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        for e in self.items.iter_mut() {
            *e = mapper.map_entity(*e);
        }
    }
}

impl History {
    pub fn new(past: impl IntoIterator<Item = Entity>) -> Self {
        let actions = VecDeque::from_iter(past);
        Self {
            index: actions.len(),
            items: actions,
        }
    }

    /// Go back one step in the history, returning the [`Entity`] of the [`HistoryItem`].
    pub fn back(&mut self) -> Option<Entity> {
        if self.index > 0 {
            self.index -= 1;
            Some(self.items[self.index])
        } else {
            None
        }
    }

    /// Go forward one step in the history, returning the [`Entity`] of the [`HistoryItem`].
    pub fn forward(&mut self) -> Option<Entity> {
        if self.index < self.items.len() {
            let entity = self.items[self.index];
            self.index += 1;
            Some(entity)
        } else {
            None
        }
    }

    /// Push an item to the past, clearing the future history.
    ///
    /// Note: the returned entities should be despawned.
    pub fn push(&mut self, entity: Entity) -> Vec<Entity> {
        let removed = self.items.drain(self.index..).collect();
        self.items.push_back(entity);
        self.index += 1;
        removed
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HistoryItem {
    undo: fn(&mut World, Entity),
    redo: fn(&mut World, Entity),
}

impl<T: Action> FromType<T> for HistoryItem {
    fn from_type() -> Self {
        Self {
            undo: |world, entity| {
                let mut action = std::mem::replace(
                    world.get_mut::<T>(entity).unwrap().as_mut(),
                    T::placeholder(),
                );
                action.undo(world);
                *world.get_mut::<T>(entity).unwrap() = action;
            },
            redo: |world, entity| {
                let mut action = std::mem::replace(
                    world.get_mut::<T>(entity).unwrap().as_mut(),
                    T::placeholder(),
                );
                action.apply(world);
                *world.get_mut::<T>(entity).unwrap() = action;
            },
        }
    }
}

impl HistoryItem {
    pub fn new<T: Action>() -> Self {
        FromType::<T>::from_type()
    }

    pub fn undo(&self, world: &mut World, entity: Entity) {
        (self.undo)(world, entity);
    }

    pub fn redo(&self, world: &mut World, entity: Entity) {
        (self.redo)(world, entity);
    }
}
