use bevy::prelude::*;
use bevy::render::camera::ScalingMode;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

use rrpg::rhythm::{BeatmapBundle, JudgementEvent, KeyEvent, RhythmSystem};
use rrpg::{GameState, RrpgPlugins};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins(RrpgPlugins)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (rrpg::rhythm::vanish_passed_notes, update)
                .after(RhythmSystem::Judgement)
                .run_if(in_state(GameState::InBattle)),
        )
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
        transform: Transform::from_xyz(0., 8. * 10., 0.),
        ..Default::default()
    });

    let beatmap = asset_server.load("beatmaps/stop_breathing.ron");

    commands.spawn(BeatmapBundle::new(beatmap));

    next_state.set(GameState::LoadingBattle);
}

fn update(
    time: Res<Time<rrpg::rhythm::Rhythm>>,
    real_time: Res<Time<Real>>,
    mut start_events: EventReader<rrpg::audio::TrackStart>,
    mut key_events: EventReader<KeyEvent>,
    mut judgement_events: EventReader<JudgementEvent>,
) {
    for _ in start_events.read() {
        info!("track started!");
    }

    for key in key_events.read() {
        //info!("gk = {:?}", key);
    }
    for je in judgement_events.read() {
        info!("je = {:?}", je);
    }
}
