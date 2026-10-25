//! T2: the plugin must create and reuse slots across scheduled layout observations.
//! A bare geometry function cannot prove the Commands/build/bind handoff.
use super::*;
use bevy::{math::Vec2, prelude::*};

fn list(app: &mut App, count: usize, height: f32) -> Entity {
    let mut bundle = PooledListBundle::new(count, 100.0, 0.0);
    bundle.config.buffer_count = 0;
    app.world_mut()
        .spawn((
            bundle,
            Node::default(),
            ComputedNode {
                size: Vec2::new(320.0, height),
                content_size: Vec2::new(320.0, count as f32 * 100.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
        ))
        .id()
}

#[test]
fn plugin_creates_initial_slots_without_caller_estimates_or_rows() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let list = list(&mut app, 100, 150.0);
    app.update();

    let mut bound: Vec<_> = app
        .world_mut()
        .query::<&PooledListItem>()
        .iter(app.world())
        .filter_map(|row| row.data_index)
        .collect();
    bound.sort_unstable();
    assert_eq!(bound, vec![0, 1, 2]);
    assert_eq!(app.world().get::<PooledListState>(list).unwrap().visible_range, 0..3);
}

#[test]
fn changed_row_metrics_update_retained_slot_geometry_and_bindings() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let list = list(&mut app, 100, 150.0);
    app.update();
    let rows: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<PooledListItem>>()
        .iter(app.world())
        .collect();
    for &row in &rows {
        app.world_mut().entity_mut(row).remove::<NeedRebind>();
    }
    let mut config = app.world_mut().get_mut::<PooledList>(list).unwrap();
    config.item_height = 80.0;
    config.gap = 10.0;
    drop(config);
    app.update();

    for &row in &rows {
        let index = app.world().get::<PooledListItem>(row).unwrap().data_index.unwrap();
        let node = app.world().get::<Node>(row).unwrap();
        assert_eq!(node.height, Val::Px(80.0));
        assert_eq!(node.top, Val::Px(index as f32 * 90.0));
        assert_eq!(app.world().get::<NeedRebind>(row).unwrap().target_index, index);
    }
}

#[test]
fn external_index_request_accounts_for_preceding_content_and_current_scroll() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let viewport = app
        .world_mut()
        .spawn((
            Node::default(),
            ScrollPosition(Vec2::new(0.0, 50.0)),
            ComputedNode {
                size: Vec2::new(320.0, 200.0),
                content_size: Vec2::new(320.0, 1200.0),
                scroll_position: Vec2::new(0.0, 50.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(160.0, 100.0))),
        ))
        .id();
    let list = app
        .world_mut()
        .spawn((
            PooledListBundle::external(10, 100.0, 0.0, viewport, viewport),
            Node::default(),
            ComputedNode {
                size: Vec2::new(320.0, 1000.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(160.0, 550.0))),
            ChildOf(viewport),
        ))
        .id();
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 4,
    });
    app.update();
    // Header = 100, row 4 starts at 400; the current 50px offset is not added twice.
    assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 500.0);

    // A new data extent has not reached layout yet. Do not clamp to the old end.
    app.world_mut().get_mut::<PooledList>(list).unwrap().total_count = 20;
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 14,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 500.0);
    app.world_mut().get_mut::<ComputedNode>(list).unwrap().size.y = 2000.0;
    app.world_mut()
        .get_mut::<ComputedNode>(viewport)
        .unwrap()
        .content_size
        .y = 2200.0;
    app.world_mut()
        .get_mut::<ComputedNode>(viewport)
        .unwrap()
        .scroll_position
        .y = 500.0;
    app.world_mut()
        .entity_mut(list)
        .insert(UiGlobalTransform::from(bevy::math::Affine2::from_translation(
            Vec2::new(160.0, 600.0),
        )));
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 1500.0);
}

#[test]
fn recycling_a_slot_releases_old_button_press_and_descendant_focus() {
    use bevy::input_focus::InputFocus;
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let list = list(&mut app, 100, 150.0);
    app.update();
    let row = app
        .world_mut()
        .query::<(Entity, &PooledListItem)>()
        .iter(app.world())
        .find(|(_, item)| item.data_index == Some(0))
        .unwrap()
        .0;
    let child = app.world_mut().spawn((bevy::ui::Pressed, ChildOf(row))).id();
    app.insert_resource(InputFocus::from_entity(child));
    app.world_mut().get_mut::<ScrollPosition>(list).unwrap().y = 1000.0;
    app.update();
    assert_ne!(app.world().get::<PooledListItem>(row).unwrap().data_index, Some(0));
    assert_eq!(app.world().resource::<InputFocus>().get(), None);
    assert!(app.world().get::<bevy::ui::Pressed>(child).is_none());
}

#[derive(Component)]
struct BoundText(String);

#[derive(Resource, Default)]
struct RebindCount(usize);

#[test]
fn resize_and_same_length_invalidation_reuse_built_content_without_idle_work() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin)
        .init_resource::<RebindCount>()
        .add_systems(
            Update,
            (
                (|rows: Query<Entity, With<NeedBuild>>, mut commands: Commands| {
                    for row in &rows {
                        commands
                            .entity(row)
                            .insert(BoundText(String::new()))
                            .remove::<NeedBuild>();
                    }
                })
                .in_set(PooledListSystems::Build),
                (|mut rows: Query<(Entity, &NeedRebind, &mut BoundText)>,
                  mut count: ResMut<RebindCount>,
                  mut commands: Commands| {
                    for (row, rebind, mut text) in &mut rows {
                        text.0 = format!("item {}", rebind.target_index);
                        count.0 += 1;
                        commands.entity(row).remove::<NeedRebind>();
                    }
                })
                .in_set(PooledListSystems::Bind),
            ),
        );
    let list = list(&mut app, 100, 150.0);
    app.update();
    assert_eq!(app.world().resource::<RebindCount>().0, 3);
    let initial: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<BoundText>>()
        .iter(app.world())
        .collect();
    app.world_mut().get_mut::<ComputedNode>(list).unwrap().size.y = 550.0;
    app.update();
    assert_eq!(app.world().resource::<RebindCount>().0, 7);
    for row in initial {
        assert!(app.world().get::<BoundText>(row).is_some());
    }
    app.world_mut().entity_mut(list).insert(PooledListInvalidated);
    app.update();
    assert_eq!(app.world().resource::<RebindCount>().0, 14);
    app.update();
    assert_eq!(app.world().resource::<RebindCount>().0, 14);
    assert!(
        app.world_mut()
            .query::<&BoundText>()
            .iter(app.world())
            .all(|text| text.0.starts_with("item "))
    );
}

#[test]
fn resolved_destination_can_be_owned_by_a_motion_adapter() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin).add_observer(
        |mut target: On<PooledListScrollTarget>, mut positions: Query<&mut ScrollPosition>| {
            assert_eq!(target.offset, 400.0);
            // A real adapter can animate toward 400; the default must not snap over it.
            positions.get_mut(target.entity).unwrap().y = 123.0;
            target.take_over();
        },
    );
    let list = list(&mut app, 100, 150.0);
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 4,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 123.0);
}

#[test]
fn explicit_cancel_and_empty_data_end_pending_requests() {
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let list = list(&mut app, 100, 0.0);
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 4,
    });
    app.update();
    app.world_mut().trigger(CancelPooledListScroll { list_entity: list });
    app.world_mut().get_mut::<ComputedNode>(list).unwrap().size.y = 200.0;
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 0.0);
    app.world_mut().get_mut::<PooledList>(list).unwrap().total_count = 0;
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 4,
    });
    app.update();
    app.world_mut().get_mut::<PooledList>(list).unwrap().total_count = 100;
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 0.0);
}

#[test]
fn native_wheel_bubbling_cancels_a_waiting_request_at_the_viewport() {
    use bevy::{
        camera::NormalizedRenderTarget,
        input::{mouse::MouseScrollUnit, touch::TouchPhase},
        picking::{
            backend::HitData,
            events::{Pointer, Scroll},
            pointer::{Location, PointerId},
        },
        window::WindowRef,
    };
    let mut app = App::new();
    app.add_plugins(PooledListPlugin);
    let list = list(&mut app, 100, 0.0);
    let content = app.world_mut().spawn(ChildOf(list)).id();
    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 4,
    });
    app.update();
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::Window(WindowRef::Entity(window).normalize(None).unwrap()),
            position: Vec2::ZERO,
        },
        Scroll {
            unit: MouseScrollUnit::Pixel,
            x: 0.0,
            y: 10.0,
            hit: HitData::new(window, 0.0, None, None),
            phase: TouchPhase::Moved,
        },
        content,
    ));
    app.world_mut().get_mut::<ComputedNode>(list).unwrap().size.y = 200.0;
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 0.0);
}

#[test]
fn layout_resize_requests_a_followup_frame_but_settled_lists_stay_idle() {
    use bevy::window::RequestRedraw;
    let mut app = App::new();
    app.add_plugins(PooledListPlugin).add_message::<RequestRedraw>();
    list(&mut app, 100, 150.0);
    // Simulate the layout boundary after Update has observed the old viewport.
    app.add_systems(
        PostUpdate,
        (|mut once: Local<bool>, mut nodes: Query<&mut ComputedNode, With<PooledList>>| {
            if !*once {
                nodes.single_mut().unwrap().size.y = 550.0;
                *once = true;
            }
        })
        .before(bevy::ui::UiSystems::Layout),
    );
    let mut reader = app.world().resource::<Messages<RequestRedraw>>().get_cursor();
    app.update();
    assert_eq!(
        reader.read(app.world().resource::<Messages<RequestRedraw>>()).count(),
        1
    );
    app.update();
    assert_eq!(app.world_mut().query::<&PooledListItem>().iter(app.world()).count(), 7);
    assert_eq!(
        reader.read(app.world().resource::<Messages<RequestRedraw>>()).count(),
        0
    );
    app.update();
    assert_eq!(
        reader.read(app.world().resource::<Messages<RequestRedraw>>()).count(),
        0
    );
}

#[test]
fn offscreen_external_navigation_wakes_after_its_new_extent_reaches_layout() {
    use bevy::window::RequestRedraw;
    let mut app = App::new();
    app.add_plugins(PooledListPlugin)
        .init_resource::<bevy::ui::ui_surface::UiSurface>()
        .init_resource::<bevy::text::FontCx>()
        .add_systems(
            PostUpdate,
            bevy::ui::ui_layout_system.in_set(bevy::ui::UiSystems::Layout),
        );
    let viewport = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(320.0),
                height: Val::Px(200.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .id();
    app.world_mut().spawn((
        Node {
            height: Val::Px(600.0),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(viewport),
    ));
    let list = app
        .world_mut()
        .spawn((
            PooledListBundle::external(10, 100.0, 0.0, viewport, viewport),
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
            ChildOf(viewport),
        ))
        .id();
    app.update();
    app.update();
    assert_eq!(app.world().get::<PooledListState>(list).unwrap().visible_range, 0..0);
    let mut reader = app.world().resource::<Messages<RequestRedraw>>().get_cursor_current();
    app.world_mut().get_mut::<PooledList>(list).unwrap().total_count = 20;
    app.world_mut().trigger(ScrollPooledListToIndex {
        list_entity: list,
        index: 14,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 0.0);
    assert_eq!(
        reader.read(app.world().resource::<Messages<RequestRedraw>>()).count(),
        1,
        "layout made navigation ready even though the list remains outside the visible intersection"
    );
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 2000.0);
    assert_eq!(
        app.world().get::<ComputedNode>(viewport).unwrap().scroll_position.y,
        2000.0
    );
}
