#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

@group(2) @binding(0) var<uniform> material_color: vec4<f32>;
@group(2) @binding(1) var base_color_texture: texture_2d<f32>;
@group(2) @binding(2) var base_color_sampler: sampler;
@group(2) @binding(3) var<uniform> scroll_speed: f32;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // use mesh's x uv, but use world position to determine scroll
    // TODO: magic number
    let uv_int = vec2<f32>(mesh.uv.x, mesh.world_position.y / 32.);
    let uv_scroll = uv_int + vec2<f32>(0., globals.time) * scroll_speed;

    // modulo to sample within texture correctly
    let uv = vec2<f32>(uv_scroll.x - trunc(uv_scroll.x), uv_scroll.y - trunc(uv_scroll.y));

    return material_color * textureSample(base_color_texture, base_color_sampler, uv);
}
