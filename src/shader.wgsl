struct Globals {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var atlas_tex: texture_2d<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) bg_color: vec4<f32>,
    @location(4) border_color: vec4<f32>,
    // params = [corner_radius, border_width, mode (0 = SDF, 1 = Text), 0]
    @location(5) params: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rect_size: vec2<f32>,
    @location(2) bg_color: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (in.pos.x / max(globals.screen_size.x, 1.0)) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.pos.y / max(globals.screen_size.y, 1.0)) * 2.0;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.rect_size = in.rect_size;
    out.bg_color = in.bg_color;
    out.border_color = in.border_color;
    out.params = in.params;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mode = in.params.z;
    if mode >= 0.5 {
        let coverage = textureSampleLevel(atlas_tex, atlas_sampler, in.uv, 0.0).r;
        return vec4<f32>(in.bg_color.rgb, in.bg_color.a * coverage);
    }

    let half_size = in.rect_size * 0.5;
    let p = in.uv - half_size;
    let r = min(in.params.x, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;

    let outer_alpha = 1.0 - smoothstep(-0.75, 0.75, d);
    let border_width = in.params.y;
    var color = in.bg_color;
    if border_width > 0.0 {
        let inner_d = d + border_width;
        let border_mix = smoothstep(-0.75, 0.75, inner_d);
        color = mix(in.bg_color, in.border_color, border_mix);
    }

    return vec4<f32>(color.rgb, color.a * outer_alpha);
}
