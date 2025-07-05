use bevy::app::App;
use bevy_rapier2d::render::RapierDebugRenderPlugin;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(RapierDebugRenderPlugin::default());
}
