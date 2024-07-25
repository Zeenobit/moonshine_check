use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, query::QueryFilter, system::EntityCommands};
use bevy_hierarchy::DespawnRecursiveExt;
use bevy_utils::tracing::{debug, error, warn};
use moonshine_kind::prelude::*;
use moonshine_save::load::LoadSystem;

pub mod prelude {
    pub use super::{invalid, panic, purge};
    pub use super::{repair, repair_remove};
    pub use super::{repair_insert, repair_insert_default};
    pub use super::{repair_replace, repair_replace_default, repair_replace_with};
    pub use super::{Check, Valid};
}

/// An extension trait used to add checks to an [`App`].
pub trait Check {
    /// Adds a new checked requirement to this [`App`] with a given [`Policy`].
    ///
    /// # Usage
    ///
    /// All new instances of given [`Kind`] `T` will be checked against the given [`CheckFilter`] `F`.
    ///
    /// If the check succeeds, the given [`Policy`] will be invoked.
    ///
    /// # Example
    /// ```
    /// use bevy::prelude::*;
    /// use moonshine_check::prelude::*;
    ///
    /// #[derive(Bundle)]
    /// struct AppleBundle {
    ///     apple: Apple,
    ///     fresh: Fresh,
    /// }
    ///
    /// #[derive(Component)]
    /// struct Apple;
    ///
    /// #[derive(Component)]
    /// struct Fresh;
    ///
    /// let mut app = App::new();
    /// // ...
    /// app.check::<Apple, Without<Fresh>>(purge());
    /// ```
    fn check<T: Kind, F: CheckFilter>(&mut self, _: Policy) -> &mut Self;
}

impl Check for App {
    fn check<T: Kind, F: CheckFilter>(&mut self, policy: Policy) -> &mut Self {
        let filter_name = || bevy_utils::get_short_name(std::any::type_name::<F>());
        self.add_systems(
            PreUpdate,
            (move |query: Query<Instance<T>, Unchecked>,
                   check: Query<(), F>,
                   world: &World,
                   mut commands: Commands| {
                for instance in query.iter() {
                    if check.get(instance.entity()).is_err() {
                        if let Some(mut entity) = commands.get_entity(instance.entity()) {
                            entity.insert(Checked);
                            debug!("{instance:?} is valid.");
                        }
                        continue;
                    }
                    match &policy {
                        Policy::Invalid => {
                            if let Some(mut entity) = commands.get_entity(instance.entity()) {
                                entity.insert((Checked, Invalid));
                                error!("{instance:?} is invalid: {}", filter_name());
                            }
                        }
                        Policy::Purge => {
                            if let Some(entity) = commands.get_entity(instance.entity()) {
                                entity.despawn_recursive();
                                error!("{instance:?} is purged: {}", filter_name());
                            }
                        }
                        Policy::Panic => {
                            panic!("{instance:?} is strictly invalid: {}", filter_name());
                        }
                        Policy::Repair(fixer) => {
                            if commands.get_entity(instance.entity()).is_some() {
                                let entity = world.entity(instance.entity());
                                commands.entity(instance.entity()).insert(Checked);
                                fixer.fix(entity, &mut commands);
                                warn!("{instance:?} was repaired: {}", filter_name());
                            }
                        }
                    }
                }
            })
            .after(LoadSystem::Load)
            .in_set(CheckSystems),
        )
    }
}

pub trait CheckFilter: 'static + QueryFilter + Send + Sync {}

impl<F> CheckFilter for F where F: 'static + QueryFilter + Send + Sync {}

#[derive(Clone, Debug, Hash, PartialEq, Eq, SystemSet)]
pub struct CheckSystems;

/// An action to be invoked if a [`Check`] *passes*.
///
/// See [`invalid`], [`purge`], [`panic`], and [`repair`] for details.
pub enum Policy {
    /// Mark the instance as invalid.
    Invalid,
    /// Despawn the instance and all of its children.
    Purge,
    /// Panic!
    Panic,
    /// Try to repair the instance with a given [`Fixer`].
    Repair(Fixer),
}

/// A fixer to be used with a [`Policy::Repair`] to try and fix an invalid instance.
pub struct Fixer(Box<dyn Fix>);

impl Fixer {
    pub fn new(f: impl Fix) -> Self {
        Self(Box::new(f))
    }

    pub fn fix(&self, entity: EntityRef, commands: &mut Commands) {
        self.0.fix(entity, commands)
    }
}

pub trait Fix: 'static + Send + Sync {
    fn fix(&self, entity: EntityRef, commands: &mut Commands);
}

impl<F: Fn(EntityRef, &mut Commands)> Fix for F
where
    F: 'static + Send + Sync,
{
    fn fix(&self, entity: EntityRef, commands: &mut Commands) {
        self(entity, commands)
    }
}

/// Returns a [`Policy`] which despawns matching instances and all of their children.
///
/// # Usage
///
/// Use this policy with [`Valid`] to allow systems to safely ignore invalid entities.
///
/// # Example
/// ```
/// use bevy::prelude::*;
/// use moonshine_check::prelude::*;
///
/// #[derive(Bundle, Default)]
/// struct AB {
///     a: A,
///     b: B,
/// }
///
/// #[derive(Component, Default)]
/// struct A;
///
/// #[derive(Component, Default)]
/// struct B;
///
/// let mut app = App::new();
/// app.add_plugins(MinimalPlugins)
///     .check::<A, Without<B>>(invalid())
///     .add_systems(Update, update_valid);
///
/// app.world_mut().spawn(AB::default()); // OK!
/// app.world_mut().spawn(A); // Bug! `B` is missing!
/// app.update();
///
/// fn update_valid(items: Query<Entity, (With<A>, Valid)>, query: Query<&B>) {
///     for entity in items.iter() {
///         // Guaranteed:
///         assert!(query.contains(entity));
///     }
/// }
/// ```
pub fn invalid() -> Policy {
    Policy::Invalid
}

/// Returns a [`Policy`] which despawns matching instances and all of their children.
///
/// # Usage
///
/// Use this policy to remove invalid entities from the world.
///
/// # Example
/// ```
/// use bevy::prelude::*;
/// use moonshine_check::prelude::*;
///
/// #[derive(Bundle, Default)]
/// struct AB {
///     a: A,
///     b: B,
/// }
///
/// #[derive(Component, Default)]
/// struct A;
///
/// #[derive(Component, Default)]
/// struct B;
///
/// let mut app = App::new();
/// app.add_plugins(MinimalPlugins)
///     .check::<A, Without<B>>(purge())
///     .add_systems(Update, update);
///
/// app.world_mut().spawn(AB::default()); // OK!
/// app.world_mut().spawn(A); // Bug! `B` is missing!
/// app.update();
///
/// fn update(items: Query<Entity, With<A>>, query: Query<&B>) {
///     for entity in items.iter() {
///         // Guaranteed:
///         assert!(query.contains(entity));
///     }
/// }
/// ```
pub fn purge() -> Policy {
    Policy::Purge
}

/// Returns a [`Policy`] which despawns matching instances and all of their children.
///
/// # Usage
///
/// Use this policy if you want to [`panic!`] on invalid entities.
///
/// In general, you should avoid using this policy as it can make your application unstable.
/// It is recommended to use [`invalid`] or [`purge`] instead, especially in a production environment.
///
/// # Example
/// ```should_panic
/// use bevy::prelude::*;
/// use moonshine_check::prelude::*;
///
/// #[derive(Bundle, Default)]
/// struct AB {
///     a: A,
///     b: B,
/// }
///
/// #[derive(Component, Default)]
/// struct A;
///
/// #[derive(Component, Default)]
/// struct B;
///
/// let mut app = App::new();
/// app.add_plugins(MinimalPlugins)
///     .check::<A, Without<B>>(panic())
///     .add_systems(Update, update);
///
/// app.world_mut().spawn(AB::default()); // OK!
/// app.world_mut().spawn(A); // Bug! `B` is missing!
/// app.update();
///
/// fn update(items: Query<Entity, With<A>>, query: Query<&B>) {
///     // Guaranteed:
///     unreachable!();
/// }
/// ```
pub fn panic() -> Policy {
    Policy::Panic
}

/// Returns a [`Policy`] which tries to repair matching instances.
///
/// # Usage
///
/// Use this policy if the matching instances can be repaired by inserting or removing components.
/// This is especially useful to handle backwards compatibility when loading from saved data.
///
/// # Example
/// ```
/// use bevy::prelude::*;
/// use moonshine_check::prelude::*;
///
/// #[derive(Bundle, Default)]
/// struct AB {
///    a: A,
///    b: B,
/// }
///
/// #[derive(Component, Default)]
/// struct A;
///
/// #[derive(Component, Default)]
/// struct B;
///
/// let mut app = App::new();
/// app.add_plugins(MinimalPlugins)
///     .check::<A, Without<B>>(repair(|entity: EntityRef, commands: &mut Commands| {
///         commands.entity(entity.id()).insert(B);
///     }));
///
/// app.world_mut().spawn(A); // Bug! `B` is missing!
/// app.update();
///
/// fn update(items: Query<Entity, With<A>>, query: Query<&B>) {
///     for entity in items.iter() {
///         // Guaranteed:
///         assert!(query.contains(entity));
///     }
/// }
pub fn repair(f: impl Fix) -> Policy {
    Policy::Repair(Fixer::new(f))
}

pub fn repair_insert<T: Component + Clone>(component: T) -> Policy {
    repair(move |entity: EntityRef, commands: &mut Commands| {
        commands.entity(entity.id()).insert(component.clone());
    })
}

pub fn repair_insert_default<T: Component + Default>() -> Policy {
    repair(move |entity: EntityRef, commands: &mut Commands| {
        commands.entity(entity.id()).insert(T::default());
    })
}

pub fn repair_replace<T: Component, U: Component + Clone>(component: U) -> Policy {
    repair(move |entity: EntityRef, commands: &mut Commands| {
        commands
            .entity(entity.id())
            .remove::<T>()
            .insert(component.clone());
    })
}

pub fn repair_replace_default<T: Component, U: Component + Default>() -> Policy {
    repair(move |entity: EntityRef, commands: &mut Commands| {
        commands
            .entity(entity.id())
            .remove::<T>()
            .insert(U::default());
    })
}

pub fn repair_replace_with<T: Component, U: Component, F>(f: F) -> Policy
where
    F: 'static + Fn(&T) -> U + Send + Sync,
{
    repair(move |entity: EntityRef, commands: &mut Commands| {
        let component = entity.get::<T>().unwrap();
        commands
            .entity(entity.id())
            .remove::<T>()
            .insert(f(component));
    })
}

pub fn repair_remove<T: Component>() -> Policy {
    repair(move |entity: EntityRef, commands: &mut Commands| {
        commands.entity(entity.id()).remove::<T>();
    })
}

/// A [`QueryFilter`] which indicates that an [`Entity`] has been checked and is valid.
///
/// See [`invalid`] for a usage example.
#[derive(QueryFilter)]
pub struct Valid(With<Checked>, Without<Invalid>);

/// An extension trait used to force an [`Entity`] to be checked again.
pub trait CheckAgain {
    fn check_again(self) -> Self;
}

impl CheckAgain for &mut EntityCommands<'_> {
    fn check_again(self) -> Self {
        self.remove::<Checked>().remove::<Invalid>()
    }
}

impl CheckAgain for &mut EntityWorldMut<'_> {
    fn check_again(self) -> Self {
        self.remove::<Checked>().remove::<Invalid>()
    }
}

type Unchecked = Without<Checked>;

#[derive(Component)]
struct Checked;

#[derive(Component)]
struct Invalid;

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

    #[derive(Component)]
    struct Foo;

    #[derive(Component)]
    struct Bar;

    #[test]
    fn test_valid() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(panic());

        let entity = app.world_mut().spawn((Foo, Bar)).id();
        app.update();

        assert!(app.world().entity(entity).contains::<Checked>());
        assert!(!app.world().entity(entity).contains::<Invalid>());
    }

    #[test]
    fn test_invalid() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(invalid());

        let entity = app.world_mut().spawn(Foo).id();
        app.update();

        assert!(app.world().entity(entity).contains::<Checked>());
        assert!(app.world().entity(entity).contains::<Invalid>());
    }

    #[test]
    fn test_purge() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(purge());

        let entity = app.world_mut().spawn(Foo).id();
        app.update();

        assert!(app.world().get_entity(entity).is_none());
    }

    #[test]
    #[should_panic]
    fn test_panic() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(panic());

        app.world_mut().spawn(Foo);
        app.update();
    }

    #[test]
    fn test_repair() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(repair(|entity: EntityRef, commands: &mut Commands| {
                commands.entity(entity.id()).insert(Bar);
            }));

        let entity = app.world_mut().spawn(Foo).id();
        app.update();

        assert!(app.world().entity(entity).contains::<Bar>());
        assert!(app.world().entity(entity).contains::<Checked>());
    }

    #[test]
    #[should_panic]
    fn test_check_again() {
        #[derive(Component)]
        struct Repaired;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(repair(|entity: EntityRef, commands: &mut Commands| {
                // Avoid infinite repair loop
                if entity.contains::<Repaired>() {
                    panic!("Bar is still missing!");
                }

                // Oops! Maybe we forget to insert Bar ...
                // Check again to be sure:
                commands.entity(entity.id()).insert(Repaired).check_again();
            }));

        let entity = app.world_mut().spawn(Foo).id();
        app.update();

        assert!(!app.world().entity(entity).contains::<Bar>());
        assert!(!app.world().entity(entity).contains::<Checked>());

        app.update(); // Should panic!
    }

    #[test]
    #[should_panic]
    fn test_multiple() {
        #[derive(Component)]
        struct Baz;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .check::<Foo, Without<Bar>>(panic())
            .check::<Foo, Without<Baz>>(panic());

        app.world_mut().spawn((Foo, Bar));
        app.update();
    }
}
