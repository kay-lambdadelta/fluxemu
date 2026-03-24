struct Uniforms {
    viewport_size: vec2<f32>,
    framebuffer_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var image: texture_2d<f32>;

@group(0) @binding(2)
var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var position = array<vec2<f32>, 3>(
        vec2<f32>(-1.0,-1.0),
        vec2<f32>(3.0,-1.0),
        vec2<f32>(-1.0,3.0),
    );

    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(position[vertex_index], 0.0, 1.0);
    out.uv = uv[vertex_index];

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let viewport_aspect = uniforms.viewport_size.x / uniforms.viewport_size.y;
    let image_aspect = uniforms.framebuffer_size.x / uniforms.framebuffer_size.y;

    var uv = in.uv;

    if (viewport_aspect > image_aspect) {
        let scale = image_aspect / viewport_aspect;
        uv.x = (uv.x - 0.5) / scale + 0.5;
    } else {
        let scale = viewport_aspect / image_aspect;
        uv.y = (uv.y - 0.5) / scale + 0.5;
    }

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    return textureSample(image, image_sampler, uv);
}
