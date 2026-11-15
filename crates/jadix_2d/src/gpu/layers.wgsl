override extended:bool=false;
@group(0) @binding(0) var backdrop: texture_2d<f32>;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(3) var layer_mask: texture_2d<f32>;
struct Params {opacity:f32, mode:f32, straight:f32, compose:f32, clip:vec4<f32>, origin:vec4<f32>};
@group(0) @binding(2) var<uniform> params:Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    return vec4<f32>(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0,0.0,1.0);
}
fn lum(c:vec3<f32>)->f32 {return dot(c,vec3<f32>(0.3,0.59,0.11));}
fn sat(c:vec3<f32>)->f32 {return max(c.r,max(c.g,c.b))-min(c.r,min(c.g,c.b));}
fn set_lum(color:vec3<f32>, value:f32)->vec3<f32> {
    var c=color+vec3<f32>(value-lum(color));
    let l=lum(c); let lo=min(c.r,min(c.g,c.b)); let hi=max(c.r,max(c.g,c.b));
    if(lo<0.0) {c=vec3<f32>(l)+(c-vec3<f32>(l))*l/max(l-lo,0.000000000001);}
    if(hi>1.0) {c=vec3<f32>(l)+(c-vec3<f32>(l))*(1.0-l)/max(hi-l,0.000000000001);}
    return clamp(c,vec3<f32>(0.0),vec3<f32>(1.0));
}
fn set_sat(c:vec3<f32>, value:f32)->vec3<f32> {
    let lo=min(c.r,min(c.g,c.b)); let range=sat(c);
    if(range<=0.0) {return vec3<f32>(0.0);}
    return (c-vec3<f32>(lo))*value/range;
}
fn mix_color(cb:vec3<f32>,cs:vec3<f32>,mode:u32)->vec3<f32> {
    switch mode {
        case 1u:{return cs*cb;}
        case 2u:{return cs+cb-cs*cb;}
        case 3u:{return select(1.0-2.0*(1.0-cs)*(1.0-cb),2.0*cs*cb,cb<=vec3<f32>(0.5));}
        case 4u:{return min(cs,cb);}
        case 5u:{return max(cs,cb);}
        case 6u:{return abs(cs-cb);}
        case 7u:{return cs+cb-2.0*cs*cb;}
        case 8u:{return select(select(min(vec3<f32>(1.0),cb/max(1.0-cs,vec3<f32>(0.000000000001))),vec3<f32>(1.0),cs>=vec3<f32>(1.0)),vec3<f32>(0.0),cb<=vec3<f32>(0.0));}
        case 9u:{return select(select(1.0-min(vec3<f32>(1.0),(1.0-cb)/max(cs,vec3<f32>(0.000000000001))),vec3<f32>(0.0),cs<=vec3<f32>(0.0)),vec3<f32>(1.0),cb>=vec3<f32>(1.0));}
        case 10u:{return select(1.0-2.0*(1.0-cs)*(1.0-cb),2.0*cs*cb,cs<=vec3<f32>(0.5));}
        case 11u:{
            let d=select(sqrt(cb),((16.0*cb-12.0)*cb+4.0)*cb,cb<=vec3<f32>(0.25));
            return select(cb+(2.0*cs-1.0)*(d-cb),cb-(1.0-2.0*cs)*cb*(1.0-cb),cs<=vec3<f32>(0.5));
        }
        case 12u:{return set_lum(set_sat(cs,sat(cb)),lum(cb));}
        case 13u:{return set_lum(set_sat(cb,sat(cs)),lum(cb));}
        case 14u:{return set_lum(cs,lum(cb));}
        case 15u:{return set_lum(cb,lum(cs));}
        default:{return cs;}
    }
}
fn source_pixel(at:vec2<i32>)->vec4<f32> {
 let q=at-vec2<i32>(params.origin.xy);let size=vec2<i32>(textureDimensions(source));
 if(any(q<vec2<i32>(0))||any(q>=size)) {return vec4<f32>(0.);}
 return textureLoad(source,q,0);
}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let at=vec2<i32>(p.xy); let s=source_pixel(at);
    if(params.mode>99.0) {
        if(params.opacity<=0.0) {return vec4<f32>(0.0);}
        if(params.straight>0.5 && s.a>0.000001) {return vec4<f32>(s.rgb/s.a,s.a*params.opacity);}
        return s*params.opacity;
    }
    let d=textureLoad(backdrop,at,0);
    if(params.compose>13.5) {
        let l=clamp(dot(s.rgb,vec3<f32>(0.2126,0.7152,0.0722))*params.opacity,0.0,1.0);
        let factor=select(l,1.0-l,params.compose>14.5);
        let xy=vec2<f32>(at);let hi=params.clip.xy+params.clip.zw;
        let extent=clamp(min(xy+vec2<f32>(1.0),hi)-max(xy,params.clip.xy),vec2<f32>(0.0),vec2<f32>(1.0));
        let ms=vec2<i32>(textureDimensions(layer_mask));
        let mask=textureLoad(layer_mask,clamp(at,vec2<i32>(0),ms-vec2<i32>(1)),0).r;
        return d*mix(1.0,factor,extent.x*extent.y*mask);
    }
    let sa=clamp(s.a,0.0,1.0)*params.opacity; let da=clamp(d.a,0.0,1.0);
    let cs=clamp(s.rgb/max(s.a,0.000001),vec3<f32>(0.0),vec3<f32>(1.0));
    let cb=clamp(d.rgb/max(d.a,0.000001),vec3<f32>(0.0),vec3<f32>(1.0));
    let mixed=mix_color(cb,cs,u32(params.mode));
    var f=vec2<f32>(1.0,1.0-sa);
    switch u32(params.compose) {
        case 0u:{f=vec2<f32>(0.0);}
        case 1u:{f=vec2<f32>(1.0,0.0);}
        case 2u:{f=vec2<f32>(0.0,1.0);}
        case 4u:{f=vec2<f32>(1.0-da,1.0);}
        case 5u:{f=vec2<f32>(da,0.0);}
        case 6u:{f=vec2<f32>(0.0,sa);}
        case 7u:{f=vec2<f32>(1.0-da,0.0);}
        case 8u:{f=vec2<f32>(0.0,1.0-sa);}
        case 9u:{f=vec2<f32>(da,1.0-sa);}
        case 10u:{f=vec2<f32>(1.0-da,sa);}
        case 11u:{f=vec2<f32>(1.0-da,1.0-sa);}
        case 12u,13u:{f=vec2<f32>(1.0);}
        default:{}
    }
    var composed=clamp(vec4<f32>(sa*f.x*((1.0-da)*cs+da*mixed)+f.y*d.rgb,sa*f.x+da*f.y),vec4<f32>(0.0),vec4<f32>(1.0));
    if(extended && params.mode<0.5) {
        composed=vec4<f32>(s.rgb*params.opacity*f.x+d.rgb*f.y,clamp(sa*f.x+da*f.y,0.0,1.0));
    }
    let xy=vec2<f32>(at); let hi=params.clip.xy+params.clip.zw;
    let extent=clamp(min(xy+vec2<f32>(1.0),hi)-max(xy,params.clip.xy),vec2<f32>(0.0),vec2<f32>(1.0));
    let ms=vec2<i32>(textureDimensions(layer_mask));
    let mask=textureLoad(layer_mask,clamp(at,vec2<i32>(0),ms-vec2<i32>(1)),0).r;
    return mix(d,composed,extent.x*extent.y*mask);
}
