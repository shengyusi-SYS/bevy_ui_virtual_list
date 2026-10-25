//! Standalone Bevy UI: native wheel input, two viewport modes, reusable content.
use bevy::{prelude::*, ui_widgets::ScrollArea};
use bevy_ui_virtual_list::*;

#[derive(Resource, Default)]
struct DataVersion(usize);

fn main() {
    demo_app().run();
}

fn demo_app() -> App {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, PooledListPlugin))
        .init_resource::<DataVersion>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                controls.before(PooledListRangeUpdate),
                build_rows.in_set(PooledListSystems::Build),
                bind_rows.in_set(PooledListSystems::Bind),
            ),
        );
    app
}

fn viewport() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(20.0)),
                row_gap: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Pooled lists | wheel to scroll | resize the window"),
                TextFont::from_font_size(24.0),
            ));
            root.spawn(Text::new(
                "S: shorten/restore  |  R: replace data  |  Enter: row 80  |  Esc: cancel pending",
            ));
            root.spawn((Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(20.0),
                ..default()
            },))
                .with_children(|body| {
                    for external in [false, true] {
                        body.spawn(Node {
                            flex_basis: Val::Px(0.0),
                            flex_grow: 1.0,
                            min_width: Val::Px(0.0),
                            height: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|pane| {
                            pane.spawn(Text::new(if external {
                                "External viewport + header/footer"
                            } else {
                                "Self-owned viewport"
                            }));
                            if external {
                                let mut outer = pane.spawn((viewport(), ScrollArea, Name::new("ExternalViewport")));
                                let owner = outer.id();
                                outer.with_children(|content| {
                                    content.spawn((
                                        Node {
                                            height: Val::Px(100.0),
                                            flex_shrink: 0.0,
                                            ..default()
                                        },
                                        Text::new("Header (100px before the list)"),
                                    ));
                                    content.spawn((
                                        PooledListBundle::external(1_000, 40.0, 4.0, owner, owner),
                                        Node {
                                            width: Val::Percent(100.0),
                                            ..default()
                                        },
                                        Name::new("ExternalList"),
                                    ));
                                    content.spawn((
                                        Node {
                                            height: Val::Px(100.0),
                                            flex_shrink: 0.0,
                                            ..default()
                                        },
                                        Text::new("Footer"),
                                    ));
                                });
                            } else {
                                pane.spawn((
                                    viewport(),
                                    ScrollArea,
                                    PooledListBundle::new(1_000, 40.0, 4.0),
                                    Name::new("SelfOwnedList"),
                                ));
                            }
                        });
                    }
                });
        });
}

fn build_rows(rows: Query<Entity, With<NeedBuild>>, mut commands: Commands) {
    for row in &rows {
        commands
            .entity(row)
            .insert(BackgroundColor(Color::srgb(0.12, 0.17, 0.23)))
            .with_child((Text::default(), TextFont::from_font_size(18.0)))
            .remove::<NeedBuild>();
    }
}

fn bind_rows(
    rows: Query<(Entity, &NeedRebind, &Children)>,
    mut texts: Query<&mut Text>,
    version: Res<DataVersion>,
    mut commands: Commands,
) {
    for (row, binding, children) in &rows {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = format!("Row {} · data v{} · slot {}", binding.target_index, version.0, row);
            }
        }
        commands.entity(row).remove::<NeedRebind>();
    }
}

fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut lists: Query<(Entity, &mut PooledList)>,
    mut version: ResMut<DataVersion>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        version.0 += 1;
    }
    for (list, mut config) in &mut lists {
        if keys.just_pressed(KeyCode::KeyS) {
            config.total_count = if config.total_count > 12 { 12 } else { 1_000 };
        }
        if keys.just_pressed(KeyCode::KeyR) {
            commands.entity(list).insert(PooledListInvalidated);
        }
        if keys.just_pressed(KeyCode::Enter) {
            commands.trigger(ScrollPooledListToIndex {
                list_entity: list,
                index: 80,
            });
        }
        if keys.just_pressed(KeyCode::Escape) {
            commands.trigger(CancelPooledListScroll { list_entity: list });
        }
    }
}
