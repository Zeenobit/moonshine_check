# ✅ Moonshine Check

[![crates.io](https://img.shields.io/crates/v/moonshine-check)](https://crates.io/crates/moonshine-check)
[![downloads](https://img.shields.io/crates/dr/moonshine-check?label=downloads)](https://crates.io/crates/moonshine-check)
[![docs.rs](https://docs.rs/moonshine-check/badge.svg)](https://docs.rs/moonshine-check)
[![license](https://img.shields.io/crates/l/moonshine-check)](https://github.com/Zeenobit/moonshine_check/blob/main/LICENSE)
[![stars](https://img.shields.io/github/stars/Zeenobit/moonshine_check)](https://github.com/Zeenobit/moonshine_check)

Validation and recovery solution for [Bevy](https://github.com/bevyengine/bevy).

## Overview

A common source of bugs in Bevy applications is invalid assumptions about the state of the world. Typically, this results in queries that "miss" their target entities due to query mismatch:

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Person;

let mut app = App::new();
app.add_systems(Update, (bad_system, unsafe_system));

fn bad_system(mut commands: Commands) {
    commands.spawn().insert(Person); // Bug: Name is missing.
}

// This system will silently skip over any `Person` entities without `Name`:
fn unsafe_system(people: Query<(&Person, &Name)>) {
    for (person, name) in people.iter() {
        println!("{:?}", name);
    }
}
```

While this example is trivial, this problem gets much worse for interdependent systems that must rely on some invariants to be able to function correctly together.

There are various solutions to this problem. Some solutions include:
1. Hiding components inside bundles to ensure they are always inserted together.
2. Using [`Expect<T>`](https://docs.rs/moonshine-util/latest/moonshine_util/expect/struct.Expect.html) in system queries.
3. Using [`Kind`](https://docs.rs/moonshine-kind/latest/moonshine_kind/trait.Kind.html) semantics to enforce requirements between system boundaries.

While these solutions are all valid, they each have flaws:
1. Bundles cannot overlap, and there is no guarantee that the bundle components will never be removed.
2. Excessive use of `Expect<T>` can lead to lots of crashes, which nobody likes.
3. `Kind` semantics are only enforced at the time of query and require manual validation if needed.

This crate offers a "last resort" solution to fully guarantee invariants in your application.
It provides a standard way to check entities for correctness and allows you to handle failures gracefully:

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct Person;

let mut app = App::new();
// Check for `Person` entities without `Name` and purge them (despawn recursively):
app.check::<Person, Without<Name>>(purge());
app.add_systems(Update, (bad_system, safe_system));

fn bad_system(mut commands: Commands) {
    // Because of the check, this entity will be purged before the next frame:
    commands.spawn().insert(Person); // Bug: Name is missing.

}

// This system will never skip a `Person` ever again!
fn safe_system(people: Query<(&Person, &Name)>) {
    for (person, name) in people.iter() {
        println!("{:?}", name);
    }
}
```

## Usage

### Check

The `check` method is used to add a new check to the application:

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

let mut app = App::new();
// ...
app.check::<A, Without<B>>(purge());
```

This function takes a [`Kind`], a [`QueryFilter`](https://docs.rs/bevy/latest/bevy/ecs/query/trait.QueryFilter.html), and a [`Policy`].

Internally, it adds a system which applies the given policy to all new entities of the given kind which match the given filter.

Entities that do **NOT** match the query are considered **Valid**.

Once an entity is checked, it will not be checked again unless manually requested (see [`check_again`]).

## Policies

There are 4 possible ways to recover from an invalid entity:

### 1. [`invalid()`](https://docs.rs/moonshine-check/latest/moonshine_check/fn.invalid.html)

This policy marks the entity as invalid and generates an error message.
This, combined with [`Valid`] allows you to define fault-tolerant systems:

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

let mut app = App::new();
// ...
app.check::<A, Without<B>>(invalid());
app.world_mut().spawn(A); // Bug!

// Pass `Valid` to your system query:
fn safe_system(query: Query<(Entity, &A), Valid>, b: Query<&B>) {
    for (entity, a) in query.iter() {
        // Safe:
        let b = b.get(entity).unwrap();
    }
}
```

### 2. [`purge()`](https://docs.rs/moonshine-check/latest/moonshine_check/fn.purge.html)

This policy despawns the entity with all of its children and generates an error message.
This prevents it from being queried by any system.

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

let mut app = App::new();
// ...
app.check::<A, Without<B>>(purge());
app.world_mut().spawn(A); // Bug!

// No need for `Valid`:
fn safe_system(query: Query<(Entity, &A)>, b: Query<&B>) {
    for (entity, a) in query.iter() {
        // Safe:
        let b = b.get(entity).unwrap();
    }
}
```

### 3. [`panic()`](https://docs.rs/moonshine-check/latest/moonshine_check/fn.panic.html)

This policy generates an error messages and stops program execution immediately.

This is mainly useful for debugging and should be avoided in a production environment.
It is equivalent to burning the whole world down because your coffee is too hot! ☕🔥

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

let mut app = App::new();
// ...
app.check::<A, Without<B>>(panic());
app.world_mut().spawn(A); // Bug!

// No need for `Valid`:
fn safe_system(query: Query<(Entity, &A)>, b: Query<&B>) {
    // Doesn't matter, we'll never get here...
    unreachable!();
}
```

### 4. [`repair(f)`](https://docs.rs/moonshine-check/latest/moonshine_check/fn.repair.html)

This policy generates a warning message and attempts to repair the entity with a given [`Fixer`].

This is useful when you can automatically restore the invalid entity into a valid state. For example, it's just missing a random marker component. There is no need to make a big fuss about it.

This policy is also useful for backwards compatibility, as it may be used to automatically upgrade saved entities to a new version.

> ✨ This crate is specifically designed to work with [`moonshine-save`](https://crates.io/crates/moonshine-save). All check systems are inserted after [`LoadSystem::Load`](https://docs.rs/moonshine-save/latest/moonshine_save/load/enum.LoadSystem.html) to ensure loaded data is always valid. 👍

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

let mut app = App::new();
// ...
app.check::<A, Without<B>>(repair(|entity, commands| {
    // It's fine, we can fix it! :D
    commands.entity(entity).insert(B);
}));

app.world_mut().spawn(A); // Bug!

// No need for `Valid`:
fn safe_system(query: Query<(Entity, &A)>, b: Query<&B>) {
    for (entity, a) in query.iter() {
        // Safe:
        let b = b.get(entity).unwrap();
    }
}
```

### Guidelines and Limitations

Checks are not free. Each check adds a new system to your application, which may impact your performance.

Avoid using checks excessively. Instead, try to check broad assumptions that are critical for application correctness. For example, you may use checks to ensure all [`Kind`] casts are valid:

```rust
use bevy::prelude::*;
use moonshine_check::prelude::*;
use moonshine_kind::prelude::*;

#[derive(Component)]
struct Fruit;

#[derive(Component)]
struct Apple;

#[derive(Bundle)]
struct AppleBundle {
    fruit: Fruit,
    apple: Apple,
}

kind!(Apple is Fruit); // Enforced by the bundle

let mut app = App::new();
// ...
app.check::<Apple, Without<Fruit>>(purge()); // Encorced by checking
```

Remember that once an entity is checked, it will not be checked again unless explicitly required. This means if any of the invariants are changed after the entity was checked, it will not be detected.

You can force an entity to be checked again by calling `check_again`:

```rust,ignore
commands.entity(entity).check_again();
```

You may also use `check_again` inside a [`Fixer`] to ensure the entity is actually fixed after the repair:

```rust,ignore
app.check::<A, Without<B>>(repair(|entity, commands| {
    // ...
    // Did we actually fix it? Not sure? Check again!
    commands.entity(entity).check_again();
}));
```

Additionally, it is recommended to group your checks into 2 broad categories:

#### Debug Checks

These checks should be reserved for internal validation, debugging, and testing.

You should consider using Rust feature flags to disable these checks for targets where performance is critical:

```rust,ignore
#[cfg(feature = "debug_checks")]
app.check::<A, (Without<B>, Without<B2>)>(panic());
```

#### Runtime Checks

These checks should be reserved for validating external input, such as deserialized, network, or user-generated data.

```rust,ignore
app.check::<A, With<B>>(repair(|entity, commands| {
    // Update B -> B2
    commands.entity(entity).remove::<B>();
    commands.entity(entity).insert(B2::default());
}));
```



[`Kind`]:https://docs.rs/moonshine-kind/latest/moonshine_kind/trait.Kind.html
[`Policy`]:https://docs.rs/moonshine-check/latest/moonshine_check/struct.Policy.html
[`Valid`]:https://docs.rs/moonshine-check/latest/moonshine_check/struct.Valid.html
[`Fixer`]:https://docs.rs/moonshine-check/latest/moonshine_check/trait.Fix.html
[`check_again`]:https://docs.rs/moonshine-check/latest/moonshine_check/trait.CheckAgain.html#method.check_again