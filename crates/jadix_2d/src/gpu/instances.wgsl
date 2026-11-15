override linear_colors:bool=false;
@group(0) @binding(0) var<uniform> globals:vec4<f32>;
struct Out {@builtin(position) p:vec4<f32>,@location(0) c:vec4<f32>};
@vertex fn vs(@location(0) p:vec2<f32>,@location(1) c:vec4<f32>,@location(2) m:vec4<f32>,@location(3) t:vec3<f32>)->Out {
    let q=vec2<f32>(m.x*p.x+m.z*p.y+t.x,m.y*p.x+m.w*p.y+t.y);
    var rgb=c.rgb;
    if(!linear_colors) {rgb=select(pow(max((rgb+0.055)/1.055,vec3<f32>(0.0)),vec3<f32>(2.4)),rgb/12.92,rgb<=vec3<f32>(0.04045));}
    let a=c.a*t.z;var out:Out;out.p=vec4<f32>(q.x/globals.x*2.0-1.0,1.0-q.y/globals.y*2.0,0.0,1.0);out.c=vec4<f32>(rgb*a,a);return out;
}
@fragment fn fs(in:Out)->@location(0) vec4<f32> {return in.c;}
