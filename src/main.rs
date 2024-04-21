use bevy::prelude::*;

use rrpg::rhythm::RhythmExt;
use rrpg::RrpgPlugins;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RrpgPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .run();
}

fn setup(asset_server: Res<AssetServer>, mut next: ResMut<rrpg::audio::NextTrack>) {
    let song = asset_server.load("song/the_shadows.ogg");

    next.set(song);
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
