struct Light {
    position_kind: vec4f,
    direction_range: vec4f,
    color_intensity: vec4f,
    cone: vec4f,
}
struct View {
    view_projection: mat4x4f,
    camera: vec4f,
    ambient_count: vec4f,
    lights: array<Light, 8>,
}
struct Material { maps: vec4f, base: vec4f, emissive_unlit: vec4f, parameters: vec4f }
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var albedo: texture_2d<f32>;
@group(1) @binding(2) var albedo_sampler: sampler;
@group(1) @binding(3) var metallic_roughness_map: texture_2d<f32>;
@group(1) @binding(4) var metallic_roughness_sampler: sampler;
@group(1) @binding(5) var normal_map: texture_2d<f32>;
@group(1) @binding(6) var normal_sampler: sampler;
@group(1) @binding(7) var occlusion_map: texture_2d<f32>;
@group(1) @binding(8) var occlusion_sampler: sampler;
@group(1) @binding(9) var emissive_map: texture_2d<f32>;
@group(1) @binding(10) var emissive_sampler: sampler;
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
}
@vertex fn vs_main(@location(0) position: vec3f, @location(1) normal: vec3f,
    @location(2) uv: vec2f, @location(3) color: vec4f) -> VertexOutput {
    var output: VertexOutput;
    output.position = view.view_projection * vec4f(position, 1.0);
    output.world = position;
    output.normal = normal;
    output.uv = uv;
    output.color = color;
    return output;
}
@fragment fn fs_main(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    let base = textureSample(albedo, albedo_sampler, input.uv) * material.base * input.color;
    let mr = textureSample(metallic_roughness_map, metallic_roughness_sampler, input.uv);
    let emissive = textureSample(emissive_map, emissive_sampler, input.uv).rgb * material.emissive_unlit.xyz;
    let occlusion = mix(1.0, textureSample(occlusion_map, occlusion_sampler, input.uv).r, material.maps.y);
    let sampled_normal = textureSample(normal_map, normal_sampler, input.uv).xyz * 2.0 - 1.0;
    let position_x = dpdx(input.world);
    let position_y = dpdy(input.world);
    let uv_x = dpdx(input.uv);
    let uv_y = dpdy(input.uv);
    var normal = normalize(select(-input.normal, input.normal, front));
    let determinant = uv_x.x * uv_y.y - uv_x.y * uv_y.x;
    if material.maps.z > 0.5 && abs(determinant) > 0.00000001 {
        let raw_tangent = (position_x * uv_y.y - position_y * uv_x.y) / determinant;
        let raw_bitangent = (-position_x * uv_y.x + position_y * uv_x.x) / determinant;
        let projected = raw_tangent - normal * dot(normal, raw_tangent);
        if dot(projected, projected) > 0.00000001 {
            let tangent = normalize(projected);
            let bitangent = cross(normal, tangent) * sign(dot(cross(normal, tangent), raw_bitangent));
            normal = normalize(tangent * sampled_normal.x * material.maps.x + bitangent * sampled_normal.y * material.maps.x + normal * sampled_normal.z);
        }
    }
    if material.parameters.z >= 0.0 && base.a < material.parameters.z { discard; }
    let alpha = select(1.0, base.a, material.parameters.w > 0.5);
    var color = base.rgb;
    if material.emissive_unlit.w < 0.5 {
        let eye = normalize(view.camera.xyz - input.world);
        let metallic = clamp(material.parameters.x * mr.b, 0.0, 1.0);
        let roughness = clamp(material.parameters.y * mr.g, 0.04, 1.0);
        let f0 = mix(vec3f(0.04), base.rgb, metallic);
        color = base.rgb * view.ambient_count.xyz * (1.0 - metallic) * occlusion;
        for (var index = 0u; index < u32(view.ambient_count.w); index += 1u) {
            let light = view.lights[index];
            var direction = -normalize(light.direction_range.xyz);
            var attenuation = 1.0;
            if light.position_kind.w > 0.5 {
                let offset = light.position_kind.xyz - input.world;
                let distance = max(length(offset), 0.001);
                direction = offset / distance;
                attenuation = 1.0 / (distance * distance);
                if light.direction_range.w > 0.0 {
                    attenuation *= pow(clamp(1.0 - pow(distance / light.direction_range.w, 4.0), 0.0, 1.0), 2.0);
                }
                if light.position_kind.w > 1.5 {
                    let cosine = dot(-direction, normalize(light.direction_range.xyz));
                    attenuation *= clamp((cosine - light.cone.y) / max(light.cone.x - light.cone.y, 0.0001), 0.0, 1.0);
                }
            }
            let halfway = normalize(eye + direction);
            let ndl = max(dot(normal, direction), 0.0);
            let ndv = max(dot(normal, eye), 0.001);
            let ndh = max(dot(normal, halfway), 0.0);
            let vdh = max(dot(eye, halfway), 0.0);
            let squared = roughness * roughness * roughness * roughness;
            let denominator = ndh * ndh * (squared - 1.0) + 1.0;
            let distribution = squared / max(3.14159265 * denominator * denominator, 0.0001);
            let fresnel = f0 + (1.0 - f0) * pow(1.0 - vdh, 5.0);
            let geometry_factor = (roughness + 1.0) * (roughness + 1.0) / 8.0;
            let geometry = (ndv / (ndv * (1.0 - geometry_factor) + geometry_factor)) *
                (ndl / max(ndl * (1.0 - geometry_factor) + geometry_factor, 0.0001));
            let specular = distribution * geometry * fresnel / max(4.0 * ndv * ndl, 0.0001);
            let diffuse = (1.0 - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
            color += (diffuse + specular) * light.color_intensity.rgb * light.color_intensity.w * attenuation * ndl;
        }
    }
    color += emissive;
    return vec4f(color * alpha, alpha);
}
