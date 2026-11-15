@group(0) @binding(0) var image:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,y*2.-1.,0.,1.);}
@fragment fn premul(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{return textureLoad(image,vec2<i32>(p.xy),0);}
@fragment fn straight(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{let c=textureLoad(image,vec2<i32>(p.xy),0);if(c.a<=0.){return vec4(0.);}return vec4(c.rgb/c.a,c.a);}
