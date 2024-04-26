//! Contains rhythm game specific rendering constructs.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
    sprite::{Material2d, Material2dPlugin, MaterialMesh2dBundle},
    transform::TransformSystem,
};
use bevy_asset_loader::prelude::*;

use crate::{
    rhythm::{
        note::{Lane, Note, Slider, SliderRef},
        RhythmSystem, NOTE_HEIGHT, NOTE_WIDTH,
    },
    GameState,
};

/// 2d effect plugin.
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<SliderMaterial2d>::default())
            .configure_loading_state(
                LoadingStateConfig::new(GameState::LoadingBattle).load_collection::<ImageAssets>(),
            )
            .add_systems(
                PostUpdate,
                (
                    bevy::transform::systems::propagate_transforms,
                    bevy::transform::systems::sync_simple_transforms,
                )
                    .in_set(RenderSystem::TransformPropagate)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_systems(
                PostUpdate,
                (
                    spawn_slider_mesh.run_if(in_state(GameState::InBattle)),
                    change_slider_length,
                )
                    .chain()
                    .in_set(RenderSystem::SpawnSliderMesh)
                    .after(RhythmSystem::NoteUpdate)
                    .after(TransformSystem::TransformPropagate)
                    .before(RenderSystem::TransformPropagate),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum RenderSystem {
    /// This render system adds a
    /// [`bevy::transform::systems::propagate_transforms`] and
    /// [`bevy::transform::systems::sync_simple_transforms`] to get updated
    /// info on transforms when adjusting render info.
    TransformPropagate,
    /// Spawns slider meshes between notes.
    SpawnSliderMesh,
}

/// A mesh for the actual slider texture.
///
/// The end of the slider is the parent.
#[derive(Clone, Component, Debug)]
pub struct SliderMesh {
    /// The start of the slider.
    pub start: Entity,
    /// The z-offset of the slider.
    pub z_offset: f32,
}

/// A 2d material for slider meshes.
#[derive(AsBindGroup, Debug, Clone, Asset, TypePath)]
pub struct SliderMaterial2d {
    /// The tint of the slider.
    #[uniform(0)]
    pub color: Color,
    /// The base texture of the slider.
    #[texture(1)]
    #[sampler(2)]
    pub color_texture: Handle<Image>,
    /// The scroll speed of the slider texture.
    #[uniform(3)]
    pub scroll_speed: f32,
}

impl Material2d for SliderMaterial2d {
    fn fragment_shader() -> ShaderRef {
        "shaders/slider_material_2d.wgsl".into()
    }
}

/// Image assets for rendering stuff.
#[derive(AssetCollection, Resource)]
pub struct ImageAssets {
    #[asset(path = "sprites/slider_default.png")]
    pub slider_default: Handle<Image>,
}

/// Spawns a slider between a slider start note and a slider end note.
pub fn spawn_slider_mesh(
    new_sliders: Query<(Entity, &SliderRef), Added<SliderRef>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut slider_materials: ResMut<Assets<SliderMaterial2d>>,
    image_assets: Res<ImageAssets>,
    mut commands: Commands,
) {
    for (slider_entity, slider_ref) in new_sliders.iter() {
        commands
            .spawn((
                MaterialMesh2dBundle {
                    // TODO: we create a mesh for every slider?!
                    mesh: meshes.add(Rectangle::default()).into(),
                    material: slider_materials.add(SliderMaterial2d {
                        color: Color::WHITE,
                        color_texture: image_assets.slider_default.clone(),
                        scroll_speed: -0.6,
                    }),
                    ..default()
                },
                SliderMesh {
                    start: slider_entity,
                    z_offset: -0.5,
                },
            ))
            .set_parent(slider_ref.get());
    }
}

/// Changes the slider length as it gets consumed and moved.
pub fn change_slider_length(
    mut slider_meshes: Query<(&SliderMesh, &Parent, &mut Transform)>,
    slider_ends: Query<(&Note, &GlobalTransform, &Parent), With<Slider>>,
    slider_begins: Query<(&Note, &GlobalTransform), With<SliderRef>>,
    lanes: Query<(&Lane, &GlobalTransform)>,
) {
    for (slider_mesh, parent, mut transform) in slider_meshes.iter_mut() {
        // the slider can be in three states:
        // * whole, neither note of the slider has been hit or missed
        // * half, the begin note has been hit but the end note hasn't
        // * none, both notes have been hit
        let Ok((end_note, end_note_transform, end_note_parent)) = slider_ends.get(parent.get())
        else {
            continue;
        };

        let Ok((lane, lane_transform)) = lanes.get(end_note_parent.get()) else {
            continue;
        };

        let Ok((begin_note, begin_note_transform)) = slider_begins.get(slider_mesh.start) else {
            continue;
        };

        // check begin note first
        if begin_note.index() >= lane.next_note_index() {
            // the slider is whole!
            // NOTE: This algorithm only works if the globaltransform of the
            // parent has only translations.
            // get distance between notes
            let height =
                (end_note_transform.translation() - begin_note_transform.translation()).length();

            // setup slider transform
            *transform = Transform::from_xyz(0., -height / 2., slider_mesh.z_offset) // midpoint
                * Transform::from_scale(Vec3::new(NOTE_WIDTH, height, 1.));
        } else if end_note.index() >= lane.next_note_index() {
            // the slider is half!
            // calculate distance from lane base
            let lane_baseline = lane_transform.translation() + Vec3::new(0., NOTE_HEIGHT / 2., 0.);

            let height = (end_note_transform.translation() - lane_baseline).length();
            //let height = height.max(0.);

            // setup slider transform
            *transform = Transform::from_xyz(0., -height / 2., slider_mesh.z_offset) // midpoint
                * Transform::from_scale(Vec3::new(NOTE_WIDTH, height, 1.));
        }

        // do nothing otherwise
    }
}
