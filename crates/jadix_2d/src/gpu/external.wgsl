@group(0) @binding(0) var input:texture_2d<f32>;
@group(1) @binding(0) var<uniform> region:vec4<u32>;
override decode_srgb:bool=false;
override premultiply:bool=true;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{
    let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(p[i],0.,1.);
}
@fragment fn color(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{
    let pixel=textureLoad(input,vec2<i32>(p.xy)+vec2<i32>(region.xy),0);let alpha=clamp(pixel.a,0.,1.);var rgb=pixel.rgb;
    if(decode_srgb){rgb=select(pow((rgb+0.055)/1.055,vec3<f32>(2.4)),rgb/12.92,rgb<=vec3<f32>(0.04045));}
    if(premultiply){rgb*=alpha;}return vec4(rgb,alpha);
}
@fragment fn mask(@builtin(position) p:vec4<f32>)->@location(0) f32{return clamp(textureLoad(input,vec2<i32>(p.xy)+vec2<i32>(region.xy),0).a,0.,1.);}
