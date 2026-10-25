use super::*;
use bevy::{
    app::App,
    math::Vec2,
    prelude::{ChildOf, DetectChanges, Schedule, World},
};

#[test]
fn effective_scroll_matches_physical_clamp_rounding_and_scrollbar_extent() {
    let computed = ComputedNode {
        size: Vec2::splat(200.0),
        content_size: Vec2::splat(600.0),
        scrollbar_size: Vec2::splat(20.0),
        inverse_scale_factor: 0.5,
        ..Default::default()
    };
    for (requested, expected) in [(-1.0, 0.0), (123.9, 123.5), (210.0, 210.0), (500.0, 210.0)] {
        assert_eq!(
            effective_scroll_position(&ScrollPosition(Vec2::splat(requested)), &computed),
            Vec2::splat(expected)
        );
    }
}

#[test]
fn pooled_layout_shrink_keeps_last_rows_bound_for_self_and_external_scroll() {
    // T1: real Bevy layout -> pool consumer; raw gesture offset is deliberately retained.
    for external in [false, true] {
        let mut world = World::new();
        world.init_resource::<bevy::ui::ui_surface::UiSurface>();
        world.init_resource::<bevy::text::FontCx>();
        let root = world
            .spawn((
                Node {
                    width: Val::Px(320.0),
                    height: Val::Px(200.0),
                    overflow: bevy::ui::Overflow::scroll_y(),
                    flex_direction: bevy::ui::FlexDirection::Column,
                    ..Default::default()
                },
                ScrollPosition(bevy::math::Vec2::new(0.0, 1800.0)),
            ))
            .id();
        let list = if external {
            world
                .spawn((
                    Node {
                        width: Val::Px(320.0),
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                    ChildOf(root),
                ))
                .id()
        } else {
            root
        };
        world.entity_mut(list).insert((
            pooled_list_config(20, 100.0, 0.0),
            PooledListState::default(),
            if external {
                PooledListBinding::External {
                    scroll_owner: root,
                    viewport_owner: root,
                }
            } else {
                PooledListBinding::SelfOwned
            },
        ));
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                managed::ensure_list_contents,
                bevy::ui::ui_layout_system,
                pooled_list_range_update_system,
                managed::grow_slots,
            )
                .chain(),
        );
        schedule.run(&mut world);
        assert_eq!(world.get::<PooledListState>(list).unwrap().visible_range, 18..20);
        world.get_mut::<PooledList>(list).unwrap().total_count = 10;
        // First pass retires out-of-range absolute rows; the next layout can shrink.
        schedule.run(&mut world);
        schedule.run(&mut world);
        assert_eq!(world.get::<ComputedNode>(root).unwrap().scroll_position.y, 800.0);
        assert_eq!(world.get::<ScrollPosition>(root).unwrap().y, 1800.0);
        assert_eq!(world.get::<PooledListState>(list).unwrap().visible_range, 8..10);
        let mut bound: Vec<_> = world
            .query::<&PooledListItem>()
            .iter(&world)
            .filter_map(|row| row.data_index)
            .collect();
        bound.sort_unstable();
        assert_eq!(bound, vec![8, 9]);
        world.get_mut::<ScrollPosition>(root).unwrap().y = 400.0;
        schedule.run(&mut world);
        assert_eq!(world.get::<PooledListState>(list).unwrap().visible_range, 4..7);
    }
}

fn computed_node(height: f32) -> ComputedNode {
    let size = Vec2::new(320.0, height);
    let mut node = ComputedNode::default();
    node.size = size;
    node.content_size = size;
    node.unrounded_size = size;
    node.inverse_scale_factor = 1.0;
    node
}

#[test]
fn missing_rows_are_reported_even_when_the_range_is_unchanged() {
    // T1: the desired range must not be mistaken for actual row coverage.
    let mut world = World::new();
    let list = world
        .spawn((
            pooled_list_config(100, 100.0, 0.0),
            PooledListBinding::SelfOwned,
            PooledListState {
                visible_range: 0..19,
                ..Default::default()
            },
            ScrollPosition::default(),
            computed_node(1800.0),
        ))
        .id();
    for index in 0..10 {
        world.spawn((
            PooledListItem {
                data_index: Some(index),
            },
            Node::default(),
            ChildOf(list),
        ));
    }
    let mut schedule = Schedule::default();
    schedule.add_systems(pooled_list_range_update_system);
    schedule.run(&mut world);
    assert_eq!(
        world.get::<PooledListDeficit>(list).map(|gap| gap.0.clone()),
        Some((10..19).collect())
    );
}

#[test]
fn row_coverage_survives_resize_scroll_and_reuses_the_expanded_pool() {
    // T1: simulate the caller supplying requested entities; exercise the real range system.
    let mut world = World::new();
    let list = world
        .spawn((
            pooled_list_config(100, 100.0, 0.0),
            PooledListBinding::SelfOwned,
            PooledListState::default(),
            ScrollPosition::default(),
            computed_node(1800.0),
        ))
        .id();
    let mut schedule = Schedule::default();
    schedule.add_systems(pooled_list_range_update_system);
    for (height, scroll, expected) in [
        (1800.0, 0.0, 0..19),
        (400.0, 0.0, 0..5),
        (400.0, 6500.0, 65..70),
        (2400.0, 6500.0, 65..90),
        (2400.0, 7600.0, 76..100),
    ] {
        // Physical size and scale resolve to the requested logical viewport height.
        let mut computed = computed_node(height * 2.0);
        computed.inverse_scale_factor = 0.5;
        computed.content_size.y = 20000.0;
        world.entity_mut(list).insert(computed);
        world.get_mut::<ScrollPosition>(list).unwrap().y = scroll;
        schedule.run(&mut world);
        if let Some(deficit) = world.get::<PooledListDeficit>(list) {
            for index in deficit.0.clone() {
                world.spawn((
                    PooledListItem {
                        data_index: Some(index),
                    },
                    Node::default(),
                    ChildOf(list),
                ));
            }
        }
        schedule.run(&mut world);
        let mut bound: Vec<_> = world
            .query::<&PooledListItem>()
            .iter(&world)
            .filter_map(|row| row.data_index)
            .collect();
        bound.sort_unstable();
        assert_eq!(bound, expected.collect::<Vec<_>>());
        assert!(world.get::<PooledListDeficit>(list).is_none());
    }
    assert_eq!(world.query::<&PooledListItem>().iter(&world).count(), 25);
    world.clear_trackers();
    schedule.run(&mut world);
    assert!(!world.entity(list).get_ref::<PooledListState>().unwrap().is_changed());
    assert!(
        world
            .query::<bevy::prelude::Ref<Node>>()
            .iter(&world)
            .all(|node| !node.is_changed())
    );
}

fn pooled_list_config(total_count: usize, item_height: f32, gap: f32) -> PooledList {
    PooledList {
        item_height,
        total_count,
        buffer_count: 0,
        gap,
    }
}

fn spawn_self_owned_list(app: &mut App, total_count: usize, item_height: f32, gap: f32, height: f32) -> Entity {
    app.world_mut()
        .spawn((
            pooled_list_config(total_count, item_height, gap),
            PooledListBinding::SelfOwned,
            PooledListState::default(),
            ScrollPosition::default(),
            ComputedNode {
                content_size: Vec2::new(320.0, calculate_content_height(total_count, item_height, gap)),
                ..computed_node(height)
            },
        ))
        .id()
}

#[test]
fn sub_row_scroll_preserves_bindings_and_cross_row_scroll_recycles() {
    // T1: exercise the production range system with a minimal ECS row pool.
    let mut world = World::new();
    let list = world
        .spawn((
            pooled_list_config(20, 100.0, 0.0),
            PooledListBinding::SelfOwned,
            PooledListState::default(),
            ScrollPosition::default(),
            ComputedNode {
                content_size: Vec2::new(320.0, 2000.0),
                ..computed_node(150.0)
            },
        ))
        .id();
    let rows: Vec<_> = (0..3)
        .map(|_| {
            world
                .spawn((PooledListItem { data_index: None }, Node::default(), ChildOf(list)))
                .id()
        })
        .collect();
    let mut schedule = Schedule::default();
    schedule.add_systems(pooled_list_range_update_system);
    schedule.run(&mut world);
    let initial: Vec<_> = rows
        .iter()
        .map(|row| world.get::<PooledListItem>(*row).unwrap().data_index)
        .collect();
    for row in &rows {
        world.entity_mut(*row).remove::<NeedRebind>();
    }
    world.clear_trackers();
    world.get_mut::<ScrollPosition>(list).unwrap().y = 25.0;
    schedule.run(&mut world);
    assert_eq!(world.get::<PooledListState>(list).unwrap().scroll_top, 25.0);
    assert_eq!(world.get::<PooledListState>(list).unwrap().visible_range, 0..3);
    for (row, index) in rows.iter().zip(initial) {
        assert_eq!(world.get::<PooledListItem>(*row).unwrap().data_index, index);
        assert!(world.get::<NeedRebind>(*row).is_none());
        assert!(!world.entity(*row).get_ref::<Node>().unwrap().is_changed());
    }
    world.get_mut::<ScrollPosition>(list).unwrap().y = 100.0;
    schedule.run(&mut world);
    let mut indices: Vec<_> = rows
        .iter()
        .map(|row| world.get::<PooledListItem>(*row).unwrap().data_index.unwrap())
        .collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![1, 2, 3]);
    let rebound: Vec<_> = rows
        .iter()
        .filter_map(|row| world.get::<NeedRebind>(*row).map(|rebind| rebind.target_index))
        .collect();
    assert_eq!(rebound, vec![3]);
}

#[test]
fn pending_scroll_clamps_to_max_scroll_for_last_item() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = spawn_self_owned_list(&mut app, 3, 100.0, 10.0, 150.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(PendingPooledListScroll { index: usize::MAX });

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    assert_eq!(scroll_position.y, 170.0);
}

#[test]
fn pending_scroll_uses_clamped_scroll_for_visible_range() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 250.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(PendingPooledListScroll { index: 19 });

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    let state = app.world().get::<PooledListState>(entity).expect("pooled state");
    assert_eq!(scroll_position.y, 1750.0);
    assert_eq!(state.visible_range, 17..20);
}

#[test]
fn pending_scroll_keeps_last_request_wins() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 250.0);
    for index in [3, 19] {
        app.world_mut().trigger(ScrollPooledListToIndex {
            list_entity: entity,
            index,
        });
    }
    app.world_mut().flush();

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    assert_eq!(scroll_position.y, 1750.0);
    assert!(app.world().get::<PendingPooledListScroll>(entity).is_none());
}

#[test]
fn pending_scroll_waits_until_layout_ready() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = app
        .world_mut()
        .spawn((PooledListBundle::new(20, 100.0, 0.0), computed_node(0.0)))
        .id();
    let initial_range = app
        .world()
        .get::<PooledListState>(entity)
        .expect("pooled state")
        .visible_range
        .clone();
    app.world_mut()
        .entity_mut(entity)
        .insert(PendingPooledListScroll { index: 19 });

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    let state = app.world().get::<PooledListState>(entity).expect("pooled state");
    assert_eq!(scroll_position.y, 0.0);
    assert_eq!(state.visible_range, initial_range);
    assert!(app.world().get::<PendingPooledListScroll>(entity).is_some());
}

#[test]
fn pending_scroll_keeps_short_list_at_top_when_max_scroll_is_zero() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = spawn_self_owned_list(&mut app, 2, 100.0, 10.0, 400.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(PendingPooledListScroll { index: usize::MAX });

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    let state = app.world().get::<PooledListState>(entity).expect("pooled state");
    assert_eq!(scroll_position.y, 0.0);
    assert_eq!(state.visible_range, 0..2);
}

#[test]
fn new_index_request_overrides_prior_offset_without_waiting_for_range_state() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 250.0);
    app.world_mut().get_mut::<ScrollPosition>(entity).unwrap().y = 120.0;
    app.world_mut().get_mut::<PooledListState>(entity).unwrap().scroll_top = 100.0;
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: entity,
        index: 19,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(entity).unwrap().y, 1750.0);
}

#[test]
fn pending_scroll_is_canceled_by_input_intent_even_without_offset_change() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 0.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(PendingPooledListScroll { index: 19 });

    app.update();

    app.world_mut().trigger(PooledListScrollInput { scroll_owner: entity });
    let mut entity_mut = app.world_mut().entity_mut(entity);
    entity_mut.insert(computed_node(250.0));
    let mut scroll_position = entity_mut.get_mut::<ScrollPosition>().expect("scroll position");
    scroll_position.y = 500.0;
    drop(scroll_position);

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(entity).expect("scroll position");
    assert_eq!(scroll_position.y, 500.0);
    assert!(app.world().get::<PendingPooledListScroll>(entity).is_none());
}

#[test]
fn stale_pending_scroll_does_not_apply_to_rebuilt_list() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);

    let old_entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 250.0);
    app.world_mut()
        .entity_mut(old_entity)
        .insert(PendingPooledListScroll { index: 19 });
    app.world_mut().despawn(old_entity);

    let new_entity = spawn_self_owned_list(&mut app, 20, 100.0, 0.0, 250.0);

    app.update();

    let scroll_position = app.world().get::<ScrollPosition>(new_entity).expect("scroll position");
    assert_eq!(scroll_position.y, 0.0);
    assert!(app.world().get::<PendingPooledListScroll>(new_entity).is_none());
}
