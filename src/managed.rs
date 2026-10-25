use bevy::prelude::*;

use crate::{
    NeedRebind, PooledList, PooledListBinding, PooledListDeficit, calculate_content_height, calculate_item_top,
    pooled_item_container,
};

/// Optional direct child of a list on which the library places its slots.
/// The library creates a plain content node when this is absent and owns its extent.
#[derive(Component, Default)]
#[require(Node)]
pub struct PooledListContent;

/// The list owning a reusable slot. This is not a business-item identity.
#[derive(Component, Clone, Copy, Debug)]
pub struct PooledListOwner(pub Entity);

/// Build the new slot's persistent content, then remove this marker.
/// Runs before binding; scrolling reuses the content instead of rebuilding it.
#[derive(Component)]
pub struct NeedBuild;

/// Application content construction and data projection run in these ordered sets.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PooledListSystems {
    Build,
    Bind,
}

#[derive(Component)]
pub(crate) struct ContentEntity(pub Entity);

pub(crate) fn ensure_list_contents(
    lists: Query<(Entity, Ref<PooledList>, &PooledListBinding, Option<&ContentEntity>)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    contents: Query<(), With<PooledListContent>>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
) {
    for (list, config, binding, cached) in &lists {
        let existing = cached
            .map(|content| content.0)
            .filter(|entity| {
                contents.contains(*entity) && parents.get(*entity).is_ok_and(|parent| parent.parent() == list)
            })
            .or_else(|| {
                children
                    .get(list)
                    .ok()?
                    .iter()
                    .find(|entity| contents.contains(*entity))
            });
        let height = Val::Px(calculate_content_height(
            config.total_count,
            config.item_height,
            config.gap,
        ));
        let content = if let Some(content) = existing {
            if let Ok(mut node) = nodes.get_mut(content) {
                set_extent(&mut node, height);
            }
            content
        } else {
            commands
                .spawn((
                    Name::new("PooledListContent"),
                    PooledListContent,
                    Node {
                        width: Val::Percent(100.0),
                        height,
                        min_height: height,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(list),
                ))
                .id()
        };
        if cached.is_none_or(|cached| cached.0 != content) {
            commands.entity(list).insert(ContentEntity(content));
        }
        // An external list contributes its full extent to the enclosing viewport.
        // A self-owned list keeps its caller-supplied viewport constraints.
        if matches!(binding, PooledListBinding::External { .. })
            && let Ok(mut node) = nodes.get_mut(list)
        {
            set_extent(&mut node, height);
        }
    }
}

fn set_extent(node: &mut Mut<Node>, height: Val) {
    if node.height != height {
        node.height = height;
    }
    if node.min_height != height {
        node.min_height = height;
    }
    if node.flex_shrink != 0.0 {
        node.flex_shrink = 0.0;
    }
}

pub(crate) fn set_slot_geometry(node: &mut Mut<Node>, index: usize, config: &PooledList) {
    let top = Val::Px(calculate_item_top(index, config.item_height, config.gap));
    let height = Val::Px(config.item_height);
    if node.top != top {
        node.top = top;
    }
    if node.height != height {
        node.height = height;
    }
    if node.display != Display::Flex {
        node.display = Display::Flex;
    }
}

pub(crate) fn release_slot_interaction(
    slot: Entity,
    children: &Query<&Children>,
    focus: &mut Option<ResMut<bevy::input_focus::InputFocus>>,
    interactions: &mut Query<&mut Interaction>,
    commands: &mut Commands,
) {
    for entity in std::iter::once(slot).chain(children.iter_descendants(slot)) {
        if let Some(focus) = focus.as_mut()
            && focus.get() == Some(entity)
        {
            focus.clear();
        }
        if let Ok(mut interaction) = interactions.get_mut(entity)
            && *interaction != Interaction::None
        {
            *interaction = Interaction::None;
        }
        commands.entity(entity).remove::<bevy::ui::Pressed>();
    }
}

pub(crate) fn grow_slots(
    lists: Query<(Entity, &PooledList, &PooledListDeficit, &ContentEntity)>,
    mut commands: Commands,
) {
    for (list, config, deficit, content) in &lists {
        for &index in &deficit.0 {
            commands.spawn((
                pooled_item_container(index, config.item_height, config.gap),
                PooledListOwner(list),
                NeedBuild,
                NeedRebind { target_index: index },
                ChildOf(content.0),
            ));
        }
        commands.entity(list).remove::<PooledListDeficit>();
    }
}

/// Layout runs after the Update-time content adapters. Reactive windows need one
/// more frame when its observations changed; otherwise a resize can leave holes
/// until the next input. Compare values because Bevy updates some layout fields
/// with change detection bypassed. Settled lists do not request redraws.
pub(crate) fn request_layout_followup(
    lists: Query<(
        Entity,
        &PooledList,
        &PooledListBinding,
        &crate::PooledListState,
        Option<&crate::navigation::PendingPooledListScroll>,
    )>,
    positions: Query<&ScrollPosition>,
    computed: Query<&ComputedNode>,
    transforms: Query<&UiGlobalTransform>,
    mut redraw: MessageWriter<bevy::window::RequestRedraw>,
) {
    for (list, config, binding, state, pending) in &lists {
        // A wholly offscreen list can keep an empty intersection while its new
        // extent becomes ready. Wake navigation even when range metrics match.
        if pending.is_some_and(|pending| {
            crate::navigation::resolve_target(list, config, binding, pending.index, &positions, &computed, &transforms)
                .is_some()
        }) {
            redraw.write(bevy::window::RequestRedraw);
            break;
        }
        let (scroll_top, height) = match binding {
            PooledListBinding::SelfOwned => {
                let (Ok(position), Ok(view)) = (positions.get(list), computed.get(list)) else {
                    continue;
                };
                (
                    crate::effective_scroll_position(position, view).y,
                    view.size().y * view.inverse_scale_factor(),
                )
            }
            PooledListBinding::External {
                scroll_owner,
                viewport_owner,
            } => {
                let (Ok(position), Ok(view), Ok(view_gt), Ok(node), Ok(gt)) = (
                    positions.get(*scroll_owner),
                    computed.get(*viewport_owner),
                    transforms.get(*viewport_owner),
                    computed.get(list),
                    transforms.get(list),
                ) else {
                    continue;
                };
                crate::resolve_external_metrics(position.y, view, view_gt, node, gt)
            }
        };
        if state.scroll_top != scroll_top
            || state.container_height != height
            || state.visible_range
                != crate::calculate_visible_range(
                    scroll_top,
                    height,
                    config.item_height,
                    config.gap,
                    config.total_count,
                    config.buffer_count,
                )
        {
            redraw.write(bevy::window::RequestRedraw);
            break;
        }
    }
}
