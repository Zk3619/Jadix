@group(0) @binding(0) var samples:texture_2d<f32>;
@group(0) @binding(1) var<uniform> config:vec4<u32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,y*2.-1.,0.,1.);
}
@fragment fn fs_main(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let nx=i32(config.x);let ny=i32(config.z);let base=vec2<i32>(p.xy)*vec2(nx,ny);var color=vec4(0.);
    for(var y=0;y<ny;y++){for(var x=0;x<nx;x++){color+=textureLoad(samples,base+vec2(x,y),0);}}
    color/=f32(nx*ny);
    if(config.y==0u && color.a>0.) {return vec4(color.rgb/color.a,color.a);}
    return color;
}
