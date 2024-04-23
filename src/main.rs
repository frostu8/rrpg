use bevy::prelude::*;
use bevy::render::camera::ScalingMode;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

use rrpg::rhythm::BeatmapBundle;
use rrpg::rhythm::RhythmExt;
use rrpg::{GameState, RrpgPlugins};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(RrpgPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, update.run_if(in_state(GameState::InBattle)))
        .run();
}

fn setup(
    asset_server: Res<AssetServer>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    commands.spawn(Camera2dBundle {
        projection: OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical(8. * 24.),
            near: 1000.,
            far: -1000.,
            ..Default::default()
        },
        ..Default::default()
    });

    let beatmap = asset_server.load("beatmaps/the_shadows.ron");

    commands.spawn(BeatmapBundle::new(beatmap));

    next_state.set(GameState::LoadingBattle);
}

fn update(
    time: Res<Time<rrpg::rhythm::Rhythm>>,
    mut last_bn: Local<i32>,
    mut start_events: EventReader<rrpg::audio::TrackStart>,
) {
    for _ in start_events.read() {
        info!("track started!");
    }

    let bn = time.beat_number() as i32;
    if *last_bn != bn {
        info!("b# = {}", bn);
        *last_bn = bn;
    }
}
