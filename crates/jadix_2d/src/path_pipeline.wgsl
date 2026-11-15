// PathPipeline: leaf-space triangles → premul MSAA RT.
// Vertex `pos` is in leaf pixels (y down). Coverage scales premul color.
// fill_kind: 0 vertex color, 1..3 gradients, 5..7 repeating, 8 legacy Gaussian
// rect, 9 independent corners / inset, 10..12 exact vector gradients.
// Kind 8 payload retains its meaning.

struct PathVert {
    @location(0) pos: vec2<f32>,
    @location(1) cov_pad: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct PathGlobals {
    resolution: vec2<f32>,
    fill_kind: f32,
    angle: f32,
    bbox: vec4<f32>,
    center: vec2<f32>,
    radii: vec2<f32>,
    // Three f32, not vec3: preserve stop_t01 at byte 64.
    // Rust PathGlobals is 256 bytes, including the two affine rows.
    stop_n: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    stop_t01: vec4<f32>,
    stop_t45: vec4<f32>,
    c0: vec4<f32>,
    c1: vec4<f32>,
    c2: vec4<f32>,
    c3: vec4<f32>,
    c4: vec4<f32>,
    c5: vec4<f32>,
    c6: vec4<f32>,
    c7: vec4<f32>,
    brush_x:vec4<f32>,
    brush_y:vec4<f32>,
};

@group(0) @binding(0)
var<uniform> g: PathGlobals;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) coverage: f32,
    @location(2) leaf: vec2<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) brush: u32,
};

@vertex
fn vs_main(v: PathVert) -> VsOut {
    var out: VsOut;
    let rx = max(g.resolution.x, 1.0);
    let ry = max(g.resolution.y, 1.0);
    let ndc = vec2<f32>(
        v.pos.x / rx * 2.0 - 1.0,
        1.0 - v.pos.y / ry * 2.0,
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.color = v.color;
    out.coverage = v.cov_pad.x;
    out.leaf = v.pos;
    out.uv = v.cov_pad;
    out.brush = u32(max(v.cov_pad.y,0.0));
    return out;
}

fn stop_t(i: u32, g: PathGlobals) -> f32 {
    if (i < 4u) {
        return g.stop_t01[i];
    }
    return g.stop_t45[i - 4u];
}

fn stop_c(i: u32, g: PathGlobals) -> vec4<f32> {
    switch i {
        case 0u: { return g.c0; }
        case 1u: { return g.c1; }
        case 2u: { return g.c2; }
        case 3u: { return g.c3; }
        case 4u: { return g.c4; }
        case 5u: { return g.c5; }
        case 6u: { return g.c6; }
        default: { return g.c7; }
    }
}

fn linear_stop(i: u32, g: PathGlobals) -> vec4<f32> {
    let c = stop_c(i,g);
    return vec4<f32>(linearize_srgb(c.rgb),c.a);
}

fn sample_stops(t_in: f32, g: PathGlobals) -> vec4<f32> {
    let n = u32(max(g.stop_n, 1.0));
    if (n <= 1u) { return linear_stop(0u,g); }
    let first = stop_t(0u,g);
    let last = stop_t(n-1u,g);
    var t = clamp(t_in,first,last);
    if (g.fill_kind > 4.0) {
        let span = max(last-first,1e-6);
        t = first + fract((t_in-first)/span)*span;
    }
    if (t <= first) { return linear_stop(0u,g); }
    for (var i = 0u; i < 7u; i++) {
        if (i+1u >= n) { break; }
        let t0 = stop_t(i,g); let t1 = stop_t(i+1u,g);
        if (t <= t1 && t1 > t0) {
            return mix(linear_stop(i,g),linear_stop(i+1u,g),clamp((t-t0)/(t1-t0),0.0,1.0));
        }
    }
    return linear_stop(n-1u,g);
}

fn linearize_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_stencil() -> @location(0) vec4<f32> {
    // Stencil triangles never write color and have no coverage discard.
    return vec4<f32>(0.0);
}

@fragment
fn fs_solid(in: VsOut) -> @location(0) vec4<f32> {
    let alpha = in.color.a * in.coverage * clip_alpha(in.leaf);
    return vec4<f32>(linearize_srgb(in.color.rgb) * alpha, alpha);
}

// Gaussian CDF approximation shared in algebra with the CPU, verified there
// against an independent f64 2D convolution oracle.
fn normal_cdf(value:f32)->f32 {
    let x=clamp(value,-12.0,12.0)*0.7071067811865476;
    let t=1.0/(1.0+0.3275911*abs(x));
    let poly=(((((1.0614054*t-1.4531521)*t)+1.4214138)*t-0.28449672)*t+0.2548296)*t;
    return 0.5*(1.0+sign(x)*(1.0-poly*exp(-x*x)));
}
const BLUR_NODES=array<f32,12>(-0.98156065,-0.9041173,-0.76990265,-0.58731896,-0.3678315,-0.12523341,0.12523341,0.3678315,0.58731896,0.76990265,0.9041173,0.98156065);
const BLUR_WEIGHTS=array<f32,12>(0.047175337,0.10693932,0.16007833,0.20316742,0.23349254,0.24914704,0.24914704,0.23349254,0.20316742,0.16007833,0.10693932,0.047175337);
fn blur_horizontal(px:f32,w:f32,inset:f32,s:f32)->f32 {
    return normal_cdf((w-inset-px)/s)-normal_cdf((inset-px)/s);
}
fn blur_cap(p:vec2<f32>,w:f32,r:f32,s:f32,range:vec2<f32>,cy:f32)->f32 {
    let lo=max(range.x,p.y-4.0*s);let hi=min(range.y,p.y+4.0*s);
    if(hi<=lo) {return 0.0;}
    let mid=lo+(hi-lo)*0.5;let half=(hi-lo)*0.5;var sum=0.0;
    for(var i=0u;i<12u;i++) {
        let yy=mid+half*BLUR_NODES[i];
        let d=(yy-cy)/r;let inset=r*(1.0-sqrt(max(0.0,1.0-d*d)));
        let q=(yy-p.y)/s;
        sum+=BLUR_WEIGHTS[i]*exp(-0.5*q*q)/s*0.3989423*blur_horizontal(p.x,w,inset,s);
    }
    return half*sum;
}
fn rounded_blur_alpha(p:vec2<f32>,size:vec2<f32>,r:f32,s:f32)->f32 {
    if(r==0.0) {return blur_horizontal(p.x,size.x,0.0,s)*(normal_cdf((size.y-p.y)/s)-normal_cdf(-p.y/s));}
    let middle=blur_horizontal(p.x,size.x,0.0,s)*(normal_cdf((size.y-r-p.y)/s)-normal_cdf((r-p.y)/s));
    return clamp(middle+blur_cap(p,size.x,r,s,vec2<f32>(0.0,r),r)+blur_cap(p,size.x,r,s,vec2<f32>(size.y-r,size.y),size.y-r),0.0,1.0);
}
fn corner_cut(p:vec2<f32>,r:f32,s:f32)->f32 {
    if(r<=0.0) {return 0.0;}
    let lo=max(0.0,p.y-4.0*s);let hi=min(r,p.y+4.0*s);
    if(hi<=lo) {return 0.0;}
    let half=(hi-lo)*0.5;let mid=lo+half;var sum=0.0;
    for(var i=0u;i<12u;i++) {
        let yy=mid+half*BLUR_NODES[i];let d=(yy-r)/r;
        let cut=r*(1.0-sqrt(max(0.0,1.0-d*d)));let z=(yy-p.y)/s;
        sum+=half*BLUR_WEIGHTS[i]*exp(-0.5*z*z)/s*0.3989423*(normal_cdf((cut-p.x)/s)-normal_cdf(-p.x/s));
    }
    return sum;
}
fn shadow_alpha(p:vec2<f32>,g:PathGlobals)->f32 {
    let s=g.center.y;let size=g.bbox.zw;
    var a=blur_horizontal(p.x,size.x,0.0,s)*(normal_cdf((size.y-p.y)/s)-normal_cdf(-p.y/s));
    a-=corner_cut(p,g.stop_t01.x,s);
    a-=corner_cut(vec2<f32>(size.x-p.x,p.y),g.stop_t01.y,s);
    a-=corner_cut(size-p,g.stop_t01.z,s);
    a-=corner_cut(vec2<f32>(p.x,size.y-p.y),g.stop_t01.w,s);
    return clamp(select(a,1.0-a,g.angle>0.5),0.0,1.0);
}

fn gradient_angle(d:vec2<f32>)->f32 {
    // atan2(0,0) is indeterminate on some shader backends. Match Rust's
    // positive-zero center convention explicitly, independent of compilation.
    if(all(d==vec2<f32>(0.0))) {return 0.0;}
    return atan2(d.y,d.x);
}
fn shade(in: VsOut, g: PathGlobals) -> vec4<f32> {
    var color = in.color;
    if(g.fill_kind>7.5) {
        let leaf=vec3<f32>(in.leaf,1.0);
        let local=vec2<f32>(dot(g.brush_x.xyz,leaf),dot(g.brush_y.xyz,leaf))-g.bbox.xy;
        var a=rounded_blur_alpha(local,g.bbox.zw,g.center.x,g.center.y);
        if(g.fill_kind>8.5) {a=shadow_alpha(local,g);}
        let alpha=a*in.color.a*in.coverage*clip_alpha(in.leaf);
        return vec4<f32>(linearize_srgb(in.color.rgb)*alpha,alpha);
    }
    if (g.fill_kind > 0.5) {
        let bw = max(g.bbox.z, 1e-6);
        let bh = max(g.bbox.w, 1e-6);
        let leaf=vec3<f32>(in.leaf,1.0);
        let brush=vec2<f32>(dot(g.brush_x.xyz,leaf),dot(g.brush_y.xyz,leaf));
        let uv=(brush-g.bbox.xy)/vec2<f32>(bw,bh);
        var t = 0.0;
        let kind = g.fill_kind - select(0.0,4.0,g.fill_kind > 4.0);
        if (kind < 1.5) {
            let rad = g.angle * 3.14159265 / 180.0;
            let dx = cos(rad);
            let dy = sin(rad);
            let px = uv.x - 0.5;
            let py = uv.y - 0.5;
            let proj = px * dx + py * dy;
            let half = 0.5 * max(abs(dx) + abs(dy), 1e-6);
            t = (proj / half + 1.0) * 0.5;
        } else if (kind < 2.5) {
            let nx = (uv.x - g.center.x) / max(g.radii.x, 1e-6);
            let ny = (uv.y - g.center.y) / max(g.radii.y, 1e-6);
            t = sqrt(nx * nx + ny * ny);
        } else {
            // UV interpolation/division can leave a few ulps at the exact
            // conic origin. An exact-zero test alone lets the triangle or
            // backend choose an arbitrary quadrant. Define that origin as +X.
            let delta=uv-g.center;
            var angle=0.0;
            if(any(abs(delta)>vec2<f32>(0.0000002))) {
                angle=gradient_angle(delta)*180.0/3.14159265;
            }
            t = fract((angle-g.angle)/360.0);
        }
        var sampled = sample_stops(t,g);
        sampled.a = sampled.a * in.color.a;
        let alpha = sampled.a * in.coverage * clip_alpha(in.leaf);
        return vec4<f32>(sampled.rgb * alpha,alpha);
    }
    let alpha = color.a * in.coverage * clip_alpha(in.leaf);
    return vec4<f32>(linearize_srgb(color.rgb) * alpha, alpha);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> { return shade(in,g); }

@fragment
fn fs_brush(in: VsOut) -> @location(0) vec4<f32> {return brush_shade(in);}
fn brush_shade(in: VsOut) -> vec4<f32> {
    let row=i32(in.brush);
    let a=textureLoad(image_tex,vec2<i32>(0,row),0);
    let b=textureLoad(image_tex,vec2<i32>(2,row),0);
    let paint=PathGlobals(a.xy,a.z,a.w,textureLoad(image_tex,vec2<i32>(1,row),0),b.xy,b.zw,
        textureLoad(image_tex,vec2<i32>(3,row),0).x,0.0,0.0,0.0,
        textureLoad(image_tex,vec2<i32>(4,row),0),textureLoad(image_tex,vec2<i32>(5,row),0),
        textureLoad(image_tex,vec2<i32>(6,row),0),textureLoad(image_tex,vec2<i32>(7,row),0),
        textureLoad(image_tex,vec2<i32>(8,row),0),textureLoad(image_tex,vec2<i32>(9,row),0),
        textureLoad(image_tex,vec2<i32>(10,row),0),textureLoad(image_tex,vec2<i32>(11,row),0),
        textureLoad(image_tex,vec2<i32>(12,row),0),textureLoad(image_tex,vec2<i32>(13,row),0),
        textureLoad(image_tex,vec2<i32>(14,row),0),textureLoad(image_tex,vec2<i32>(15,row),0));
    if(paint.fill_kind>9.5) {return vector_shade(in,paint);}
    return shade(in,paint);
}

// Exact float stop table: two texels per stop (offset, straight linear color).
// Binary search chooses the rightmost equal offset for hard transitions.
fn vector_texel(index:u32)->vec4<f32> {
    return textureLoad(image_tex,vec2<i32>(i32(index%16u),i32(index/16u)),0);
}
fn gradient_to_srgb(rgb:vec3<f32>)->vec3<f32> {
    return select(1.055*pow(max(rgb,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,12.92*rgb,rgb<=vec3<f32>(0.0031308));
}
fn gradient_to_space(rgb:vec3<f32>, space:f32)->vec3<f32> {
    if(space<0.5) {return rgb;}
    if(space<1.5) {
        return select(1.055*pow(max(rgb,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,12.92*rgb,rgb<=vec3<f32>(0.0031308));
    }
    if(space>3.5) {
        let c=gradient_to_srgb(rgb);let hi=max(c.r,max(c.g,c.b));let lo=min(c.r,min(c.g,c.b));let d=hi-lo;let light=(hi+lo)*0.5;
        if(d<=0.0000001) {return vec3<f32>(light,0.0,0.0);}
        var h=(c.r-c.g)/d+4.0;if(hi==c.r) {h=(c.g-c.b)/d;}else if(hi==c.g) {h=(c.b-c.r)/d+2.0;}
        h*=60.0;h-=floor(h/360.0)*360.0;
        return vec3<f32>(light,d/(1.0-abs(2.0*light-1.0)),h);
    }
    let lms=pow(max(vec3<f32>(dot(rgb,vec3<f32>(0.41222146,0.53633255,0.051445995)),dot(rgb,vec3<f32>(0.2119035,0.6806995,0.10739696)),dot(rgb,vec3<f32>(0.08830246,0.28171885,0.6299787))),vec3<f32>(0.0)),vec3<f32>(1.0/3.0));
    let lab=vec3<f32>(dot(lms,vec3<f32>(0.21045426,0.7936178,-0.004072047)),dot(lms,vec3<f32>(1.9779985,-2.4285922,0.4505937)),dot(lms,vec3<f32>(0.025904037,0.78277177,-0.80867577)));
    if(space>2.5) {var h=gradient_angle(lab.yz)*180.0/3.141592653589793;h-=floor(h/360.0)*360.0;return vec3<f32>(lab.x,length(lab.yz),h);}
    return lab;
}
fn gradient_from_space(rgb:vec3<f32>, space:f32)->vec3<f32> {
    if(space<0.5) {return rgb;}
    if(space<1.5) {return linearize_srgb(rgb);}
    if(space>3.5) {
        let chroma=(1.0-abs(2.0*rgb.x-1.0))*rgb.y;var h=rgb.z-floor(rgb.z/360.0)*360.0;h/=60.0;
        let x=chroma*(1.0-abs(h-floor(h/2.0)*2.0-1.0));var c=vec3<f32>(chroma,0.0,x);
        if(h<1.0){c=vec3<f32>(chroma,x,0.0);}else if(h<2.0){c=vec3<f32>(x,chroma,0.0);}else if(h<3.0){c=vec3<f32>(0.0,chroma,x);}else if(h<4.0){c=vec3<f32>(0.0,x,chroma);}else if(h<5.0){c=vec3<f32>(x,0.0,chroma);}
        return linearize_srgb(clamp(c+rgb.x-chroma*0.5,vec3<f32>(0.0),vec3<f32>(1.0)));
    }
    var lab=rgb;
    if(space>2.5) {let angle=rgb.z*3.141592653589793/180.0;lab=vec3<f32>(rgb.x,rgb.y*cos(angle),rgb.y*sin(angle));}
    let v=vec3<f32>(dot(lab,vec3<f32>(1.0,0.39633778,0.21580376)),dot(lab,vec3<f32>(1.0,-0.105561346,-0.06385417)),dot(lab,vec3<f32>(1.0,-0.08948418,-1.2914855)));
    let p=v*v*v;
    return vec3<f32>(dot(p,vec3<f32>(4.0767417,-3.3077116,0.23096994)),dot(p,vec3<f32>(-1.268438,2.6097574,-0.34131938)),dot(p,vec3<f32>(-0.0041960863,-0.7034186,1.7076147)));
}
fn vector_stops(value:f32,g:PathGlobals)->vec4<f32> {
    var t=value;let extend=u32(g.angle);
    if(extend==1u) {t=fract(t);}
    if(extend==2u) {let p=t-floor(t/2.0)*2.0;t=select(2.0-p,p,p<=1.0);}
    if(extend==3u && (t<0.0 || t>1.0)) {return vec4<f32>(0.0);}
    t=clamp(t,0.0,1.0);
    let base=u32(g.stop_t01.x);let n=u32(g.stop_n);
    var lo=0u;var hi=n;
    for(var step=0u;step<13u;step++) {
        if(lo>=hi) {break;}
        let mid=lo+(hi-lo)/2u;
        if(vector_texel(base+mid*2u).x<=t) {lo=mid+1u;} else {hi=mid;}
    }
    let a=select(lo-1u,0u,lo==0u);let b=min(lo,n-1u);
    var ca=vector_texel(base+a*2u+1u);var cb=vector_texel(base+b*2u+1u);
    let space=g.stop_t01.z;
    if(space>0.5) {ca=vec4<f32>(gradient_to_space(ca.rgb,space),ca.a);cb=vec4<f32>(gradient_to_space(cb.rgb,space),cb.a);}
    if(space>2.5) {
        let ta=vector_texel(base+a*2u).x;let tb=vector_texel(base+b*2u).x;
        var f=0.0;if(tb>ta) {f=pow(clamp((t-ta)/(tb-ta),0.0,1.0),vector_texel(base+a*2u).y);}
        let epsilon=select(0.0000001,0.000004,space<3.5);
        if(ca.y<=epsilon) {ca.z=cb.z;}if(cb.y<=epsilon) {cb.z=ca.z;}
        var delta=cb.z-ca.z;let hue=g.stop_t45.x;
        if(hue<0.5) {if(delta>180.0){delta-=360.0;}else if(delta< -180.0){delta+=360.0;}}
        else if(hue<1.5) {if(delta>0.0&&delta<180.0){delta-=360.0;}else if(delta<=0.0&&delta> -180.0){delta+=360.0;}}
        else if(hue<2.5) {if(delta<0.0){delta+=360.0;}}
        else {if(delta>0.0){delta-=360.0;}}
        let out_alpha=mix(ca.a,cb.a,f);let out_hue=ca.z+delta*f;
        if(g.stop_t01.y<0.5) {ca.x*=ca.a;ca.y*=ca.a;cb.x*=cb.a;cb.y*=cb.a;}
        var pair=mix(ca.xy,cb.xy,f);if(g.stop_t01.y<0.5&&out_alpha>0.0){pair/=out_alpha;}
        return vec4<f32>(clamp(gradient_from_space(vec3<f32>(pair,out_hue),space),vec3<f32>(0.0),vec3<f32>(1.0))*out_alpha,out_alpha);
    }
    if(g.stop_t01.y<0.5) {ca=vec4<f32>(ca.rgb*ca.a,ca.a);cb=vec4<f32>(cb.rgb*cb.a,cb.a);}
    let ta=vector_texel(base+a*2u).x;let tb=vector_texel(base+b*2u).x;
    var f=0.0;if(tb>ta) {f=pow(clamp((t-ta)/(tb-ta),0.0,1.0),vector_texel(base+a*2u).y);}
    var c=mix(ca,cb,f);
    if(space>0.5) {
        var rgb=c.rgb;
        if(g.stop_t01.y<0.5 && c.a>0.0) {rgb=rgb/c.a;}
        c=vec4<f32>(clamp(gradient_from_space(rgb,space),vec3<f32>(0.0),vec3<f32>(1.0))*c.a,c.a);
    } else if(g.stop_t01.y>0.5) {c=vec4<f32>(c.rgb*c.a,c.a);}
    return c;
}
fn vector_parameter(p:vec2<f32>,g:PathGlobals)->vec2<f32> {
    if(g.fill_kind<10.5) {
        let d=g.bbox.zw-g.bbox.xy;
        let scale=max(abs(d.x),abs(d.y));let normalized=d/scale;
        return vec2<f32>(dot((p-g.bbox.xy)/scale,normalized)/dot(normalized,normalized),1.0);
    }
    if(g.fill_kind>11.5) {
        let d=p-g.bbox.xy;let a=gradient_angle(d);let start=g.bbox.z;let span=g.bbox.w;
        if(g.stop_t01.w>0.5) {
            let angle=a-floor(a/6.283185307179586)*6.283185307179586;
            return vec2<f32>((angle-start)/span,1.0);
        }
        let signum=select(-1.0,1.0,span>0.0);
        let raw=(a-start)*signum;let delta=raw-floor(raw/6.283185307179586)*6.283185307179586;
        return vec2<f32>(delta/abs(span),1.0);
    }
    let delta=g.bbox.zw-g.bbox.xy;let dr=g.center.y-g.center.x;
    let scale=max(max(abs(delta.x),abs(delta.y)),max(max(abs(g.center.x),abs(g.center.y)),0.00000000000000000001));
    let d=delta/scale;let q=(p-g.bbox.xy)/scale;let r=g.center.x/scale;let radius_delta=dr/scale;
    let a=dot(d,d)-radius_delta*radius_delta;
    let b=-2.0*(dot(q,d)+r*radius_delta);let c=dot(q,q)-r*r;
    if(abs(a)<=0.0000001*max(dot(d,d)+radius_delta*radius_delta,0.000000000000000000000000000001)) {
        if(b==0.0) {return vec2<f32>(0.0);}
        let t=-c/b;return vec2<f32>(t,select(0.0,1.0,r+t*radius_delta>=0.0));
    }
    let disc=b*b-4.0*a*c;if(disc<0.0) {return vec2<f32>(0.0);}
    let root=sqrt(disc);let k=-0.5*(b+select(root,-root,b<0.0));
    var t0=-b/(2.0*a);var t1=t0;if(k!=0.0) {t0=k/a;t1=c/k;}
    let v0=r+t0*radius_delta>=0.0;let v1=r+t1*radius_delta>=0.0;
    if(!v0 && !v1) {return vec2<f32>(0.0);}
    return vec2<f32>(select(select(t1,t0,v0),max(t0,t1),v0&&v1),1.0);
}
fn vector_shade(in:VsOut,g:PathGlobals)->vec4<f32> {
    let leaf=vec3<f32>(in.leaf,1.0);
    let p=vec2<f32>(dot(g.brush_x.xyz,leaf),dot(g.brush_y.xyz,leaf));
    if(!all(abs(p)<=vec2<f32>(3.402823e38))) {return vec4<f32>(0.0);}
    let parameter=vector_parameter(p,g);
    if(parameter.y<0.5 || !(abs(parameter.x)<=3.402823e38)) {return vec4<f32>(0.0);}
    return vector_stops(parameter.x,g)*in.color.a*in.coverage*clip_alpha(in.leaf);
}
@fragment fn fs_vector(in:VsOut)->@location(0) vec4<f32> {return vector_shade(in,g);}

@group(1) @binding(0)
var coverage_tex: texture_2d<f32>;

// During drawing group 1 is the immutable clip mask (1x1 white means no mask).
// The resolve pass binds its coverage target at the same layout slot.
fn clip_alpha(leaf: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(coverage_tex));
    let pos = clamp(vec2<i32>(floor(leaf)),vec2<i32>(0),size-vec2<i32>(1));
    if (size.x == 1 && size.y == 1) { return 1.0; }
    return textureLoad(coverage_tex,pos,0).r;
}

@group(2) @binding(0) var image_tex: texture_2d<f32>;
@group(2) @binding(1) var image_sampler: sampler;
// Ordinary images: per-primitive domain headers in the shared material table.
// Analytic Area pipelines use this same layout slot for triangle geometry.
@group(3) @binding(0) var geometry_data: texture_2d<f32>;

@fragment
fn fs_image(in: VsOut) -> @location(0) vec4<f32> {
    // Texture RGB is linear premultiplied before filtering; no transparent fringe.
    if(in.color.r>=0.) {
        let row=i32(in.color.r);
        let bounds=textureLoad(geometry_data,vec2<i32>(1,row),0);
        let quality=textureLoad(geometry_data,vec2<i32>(0,row),0).w;
        let plan=textureLoad(geometry_data,vec2<i32>(14,row),0);
        return reconstruction_with_plan(in.uv*vec2<f32>(textureDimensions(image_tex)),vec2(0.),quality,bounds,plan)*in.color.a*clip_alpha(in.leaf);
    }
    // The material index is per primitive; explicit LOD avoids implicit
    // derivatives after divergent control flow (strict WebGPU uniformity).
    // Image resources currently have exactly one mip level.
    return textureSampleLevel(image_tex,image_sampler,in.uv,0.) * in.color.a * clip_alpha(in.leaf);
}

// Extend each tap, including taps across repeat seams. Texture RGB is already
// linear premultiplied, so filtering transparent texels cannot introduce halos.
fn pattern_index(x:f32,n:f32,mode:u32)->f32 {
    // Explicit branches also satisfy legacy FXC's return-path analysis when
    // called from the nested reconstruction loops.
    // Integer texel wrapping avoids reciprocal-rounding errors at exact NPOT
    // multiples (e.g. 15/15 may evaluate just below one on GPU backends).
    if(mode==1u || mode==2u) {
        let period=i32(n)*select(1,2,mode==2u);
        var reduced=x;
        if(abs(x)>1000000000.) {reduced=x-floor(x/f32(period))*f32(period);}
        // One unsigned division, then restore Euclidean wrapping for negative taps.
        let rem=u32(abs(reduced))%u32(period);
        let q=i32(select(rem,u32(period)-rem,reduced<0. && rem!=0u));
        if(mode==1u) {return f32(q);}
        return f32(select(period-1-q,q,q<i32(n)));
    }
    if(mode==3u) {if(x<0.0 || x>=n) {return -1.0;}return x;}
    return clamp(x,0.0,n-1.0);
}

fn reconstruction_texel(p:vec2<f32>,mode:vec2<f32>,bounds:vec4<f32>)->vec4<f32> {
    let size=bounds.zw-bounds.xy;let at=p-bounds.xy;
    let q=vec2(pattern_index(at.x,size.x,u32(mode.x)),pattern_index(at.y,size.y,u32(mode.y)));
    if(any(q<vec2(0.))) {return vec4(0.);}
    return textureLoad(image_tex,vec2<i32>(q+bounds.xy),0);
}
fn reconstruction_weight(value:f32,quality:f32)->f32 {
    let x=abs(value);
    if(quality<2.5) {
        if(x<1.) {return (7.*x*x*x-12.*x*x+16./3.)/6.;}
        if(x<2.) {return (-7./3.*x*x*x+12.*x*x-20.*x+32./3.)/6.;}
        return 0.;
    }
    if(x<0.000001) {return 1.;}if(x>=3.) {return 0.;}
    let p=3.141592653589793*x;return sin(p)*sin(p/3.)/(p*p/3.);
}
// Premultiplied linear convolution; negative lobes cannot create invalid alpha
// or RGB outside the contributing straight-color envelope. HDR/signed RGB stays.
fn reconstruct_image(at:vec2<f32>,mode:vec2<f32>,quality:f32,bounds:vec4<f32>)->vec4<f32> {
    if(quality<0.5) {return reconstruction_texel(floor(at),mode,bounds);}
    let p=at-vec2(0.5);let f=fract(p);let q=floor(p);
    if(quality<1.5) {
        return mix(mix(reconstruction_texel(q,mode,bounds),reconstruction_texel(q+vec2(1.,0.),mode,bounds),f.x),
            mix(reconstruction_texel(q+vec2(0.,1.),mode,bounds),reconstruction_texel(q+vec2(1.),mode,bounds),f.x),f.y);
    }
    var first=-1;var last=2;if(quality>2.5) {first=-2;last=3;}
    var sum=vec4(0.);var weight=0.;var low=vec3(3.402823e38);var high=vec3(-3.402823e38);
    for(var y=first;y<=last;y++) {for(var x=first;x<=last;x++) {
        let w=reconstruction_weight(f32(x)-f.x,quality)*reconstruction_weight(f32(y)-f.y,quality);
        if(w==0.) {continue;}
        let c=reconstruction_texel(q+vec2(f32(x),f32(y)),mode,bounds);sum+=c*w;weight+=w;
        if(c.a>0.) {let rgb=c.rgb/c.a;low=min(low,rgb);high=max(high,rgb);}
    }}
    if(sum.a<=0. || weight<=0.) {return vec4(0.);}
    let alpha=clamp(sum.a/weight,0.,1.);
    return vec4(clamp(sum.rgb/sum.a,low,high)*alpha,alpha);
}
@fragment fn fs_pattern(in:VsOut)->@location(0) vec4<f32> {return pattern_shade(in);}
fn pattern_shade(in:VsOut)->vec4<f32> {
    var plan=vec4(0.,0.,0.,1.);let row=i32(floor(in.color.z/8.));
    let quality=in.color.z-f32(row)*8.;
    if(quality>=4.) {plan=textureLoad(geometry_data,vec2<i32>(14,row),0);}
    return pattern_with_plan(in,plan);
}
fn pattern_with_plan(in:VsOut,plan:vec4<f32>)->vec4<f32> {
    let bounds=vec4(vec2(0.),vec2<f32>(textureDimensions(image_tex)));
    let quality=in.color.z-floor(in.color.z/8.)*8.;
    return reconstruction_with_plan(in.uv,in.color.xy,quality,bounds,plan)*in.color.a*clip_alpha(in.leaf);
}
// Use one four-tap implementation at every mip, including level zero. Separate
// level-zero calls cloned the full extend-mode graph inside each EWA fallback.
fn mip_bilinear(at:vec2<f32>,mode:vec2<f32>,bounds:vec4<f32>,level:i32)->vec4<f32> {
    let base=vec2<f32>(textureDimensions(image_tex));let size=vec2<f32>(textureDimensions(image_tex,level));
    let p=at*size/base-vec2(0.5);let q=floor(p);let f=fract(p);
    let origin=select(vec2(0.),bounds.xy,level==0);
    let domain=select(size,bounds.zw-bounds.xy,level==0);
    var out=vec4(0.);
    for(var y=0;y<2;y++){for(var x=0;x<2;x++) {
        let tap=q+vec2(f32(x),f32(y))-origin;
        let index=vec2(pattern_index(tap.x,domain.x,u32(mode.x)),pattern_index(tap.y,domain.y,u32(mode.y)));
        if(all(index>=vec2(0.))) {
            let weight=select(1.-f.x,f.x,x==1)*select(1.-f.y,f.y,y==1);
            out+=textureLoad(image_tex,vec2<i32>(index+origin),level)*weight;
        }
    }}return out;
}
// Gaussian elliptical reconstruction over actual NPOT mip coordinates. Bounded
// support is explicit (17x17), including when callers truncate a mip chain.
fn ewa_level(at:vec2<f32>,mode:vec2<f32>,bounds:vec4<f32>,plan:vec4<f32>,level:i32)->vec4<f32> {
 let axis_length=length(plan.xy);
 if(axis_length<=1.000001 && plan.w<=1.000001) {return mip_bilinear(at,mode,bounds,level);}
 let size=vec2<f32>(textureDimensions(image_tex,level));let ratio=size/vec2<f32>(textureDimensions(image_tex));
 let direction=plan.xy/max(axis_length,0.0000001);var a=plan.xy*ratio;var b=vec2(-direction.y,direction.x)*plan.w*ratio;
 let radius=max(length(vec2(a.x,b.x)),length(vec2(a.y,b.y)));let scale=8./max(radius,8.);a*=scale;b*=scale;
 let xx=a.x*a.x+b.x*b.x;let xy=a.x*a.y+b.x*b.y;let yy=a.y*a.y+b.y*b.y;let det=xx*yy-xy*xy;
 if(det<=0.000000000001) {return mip_bilinear(at,mode,bounds,level);}
 let center=at*ratio-vec2(0.5);let origin=floor(center);var out=vec4(0.);var total=0.;
 // Uniform-dependent bounds avoid FXC expanding a fixed 289-tap kernel.
 // Candidates outside these axis bounds already have quadratic radius >=1.
 let rx=i32(min(8.,ceil(sqrt(xx))));let ry=i32(min(8.,ceil(sqrt(yy))));
 let width=2*rx+1;let count=width*(2*ry+1);
 for(var i=0;i<count;i++) {
  let x=i%width-rx;let y=i/width-ry;
  let p=origin+vec2(f32(x),f32(y));let d=p-center;let r2=(yy*d.x*d.x-2.*xy*d.x*d.y+xx*d.y*d.y)/det;
  if(r2>=0. && r2<1.) {
   let weight=exp(-2.*r2)-exp(-2.);total+=weight;
   let index=vec2(pattern_index(p.x,size.x,u32(mode.x)),pattern_index(p.y,size.y,u32(mode.y)));
   if(all(index>=vec2(0.))) {out+=textureLoad(image_tex,vec2<i32>(index),level)*weight;}
  }
 }if(total>0.) {return out/total;}return mip_bilinear(at,mode,bounds,level);
}
fn reconstruction_with_plan(at:vec2<f32>,mode:vec2<f32>,quality:f32,bounds:vec4<f32>,plan:vec4<f32>)->vec4<f32> {
    if(quality<4.) {return reconstruct_image(at,mode,quality,bounds);}
    // CPU headers clamp to the bound texture's actual mip range; WebGL2
    // cannot query textureNumLevels in GLSL ES 3.00. Ceil stays within that range.
    let lod=max(plan.z,0.);let a=i32(floor(lod));let b=i32(ceil(lod));let f=fract(lod);
    if(quality>5.5) {let value=ewa_level(at,mode,bounds,plan,a);if(a==b){return value;}return mix(value,ewa_level(at,mode,bounds,plan,b),f);}
    let count=u32(clamp(plan.w,1.,16.));var out=vec4(0.);
    for(var i=0u;i<count;i++) {
        let point=at+plan.xy*(f32(i)+0.5-f32(count)*0.5);
        out+=mix(mip_bilinear(point,mode,bounds,a),mip_bilinear(point,mode,bounds,b),f)/f32(count);
    }return out;
}

@vertex
fn vs_resolve(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let scale = vec2<i32>(round(vec2<f32>(textureDimensions(coverage_tex)) / g.resolution));
    let base = vec2<i32>(p.xy) * scale;
    var color = vec4<f32>(0.0);
    for (var y = 0; y < scale.y; y++) {
        for (var x = 0; x < scale.x; x++) {
            color += textureLoad(coverage_tex, base + vec2<i32>(x, y), 0);
        }
    }
    color /= f32(scale.x * scale.y);
    // Average linear premultiplied color/coverage, then export the straight-alpha
    // texture expected by the shared image compositor. Never blur opaque interiors.
    if (color.a <= 1e-6) {
        return vec4<f32>(0.0);
    }
    if (g._pad0 > 0.5) { return color; }
    return vec4<f32>(color.rgb / color.a, color.a);
}
