use bevy::prelude::*;

use rrpg::audio::RhythmExt;
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

fn update(time: Res<Time<rrpg::audio::Rhythm>>, mut last_bn: Local<u32>) {
    let bn = time.beat_number();
    if *last_bn != bn {
        info!("b# = {}", bn);
        *last_bn = bn;
    }
}
