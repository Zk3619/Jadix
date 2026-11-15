@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var extra: texture_2d<f32>;
struct Params { data: array<vec4<f32>,24> };
@group(0) @binding(2) var<uniform> params: Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    return vec4<f32>(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0,0.0,1.0);
}
fn cdf(value:f32)->f32 {
    let x=clamp(value,-12.0,12.0)*0.7071067811865476;
    let t=1.0/(1.0+0.3275911*abs(x));
    let poly=(((((1.0614054*t-1.4531521)*t)+1.4214138)*t-0.28449672)*t+0.2548296)*t;
    return 0.5*(1.0+sign(x)*(1.0-poly*exp(-x*x)));
}
fn load_source(at:vec2<i32>)->vec4<f32> {
    if(any(at<vec2<i32>(0)) || any(at>=vec2<i32>(textureDimensions(source)))) {return vec4<f32>(0.0);}
    return textureLoad(source,at,0);
}
fn load_alpha(at:vec2<i32>)->f32 {
    if(any(at<vec2<i32>(0)) || any(at>=vec2<i32>(textureDimensions(extra)))) {return 0.0;}
    return textureLoad(extra,at,0).a;
}
fn shadow_alpha(p:vec2<f32>)->f32 {
    if(any(p<vec2<f32>(-1.0)) || any(p>=vec2<f32>(textureDimensions(extra)))) {return 0.0;}
    let q=vec2<i32>(floor(p));let f=fract(p);
    return mix(mix(load_alpha(q),load_alpha(q+vec2<i32>(1,0)),f.x),mix(load_alpha(q+vec2<i32>(0,1)),load_alpha(q+vec2<i32>(1,1)),f.x),f.y);
}
fn sample_source(p:vec2<f32>)->vec4<f32> {
    if(any(p<vec2<f32>(-1.0)) || any(p>=vec2<f32>(textureDimensions(source)))) {return vec4<f32>(0.0);}
    let q=vec2<i32>(floor(p));let f=fract(p);
    return mix(mix(load_source(q),load_source(q+vec2<i32>(1,0)),f.x),mix(load_source(q+vec2<i32>(0,1)),load_source(q+vec2<i32>(1,1)),f.x),f.y);
}
fn scalar(i:u32)->f32 {return params.data[i/4u][i%4u];}
fn encode_rgb(c:vec3<f32>)->vec3<f32> {
    return select(1.055*pow(max(c,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,12.92*c,c<=vec3<f32>(0.0031308));
}
fn decode_rgb(c:vec3<f32>)->vec3<f32> {
    return select(pow(max((c+0.055)/1.055,vec3<f32>(0.0)),vec3<f32>(2.4)),c/12.92,c<=vec3<f32>(0.04045));
}
fn transfer_component(value:f32,base:u32)->f32 {
    let x=clamp(value,0.0,1.0);let kind=scalar(base);let a=scalar(base+1u);let b=scalar(base+2u);let c=scalar(base+3u);
    var result=x;
    if(kind>0.5 && kind<1.5) {result=a*x+b;}
    if(kind>1.5 && kind<2.5) {var power=1.0;if(b>0.0) {power=pow(x,b);}result=a*power+c;}
    if(kind>2.5 && kind<3.5) {
        let p=x*(a-1.0);let lo=u32(floor(p));let hi=min(lo+1u,u32(a)-1u);
        result=mix(scalar(base+4u+lo),scalar(base+4u+hi),fract(p));
    }
    if(kind>3.5) {result=scalar(base+4u+min(u32(x*a),u32(a)-1u));}
    return clamp(result,0.0,1.0);
}
fn convolution_load(pos:vec2<i32>)->vec4<f32> {
    let size=vec2<i32>(textureDimensions(source));var q=pos;let edge=params.data[2].z;
    if(edge<0.5 && (any(q<vec2<i32>(0)) || any(q>=size))) {return vec4<f32>(0.0);}
    if(edge>0.5 && edge<1.5) {q=clamp(q,vec2<i32>(0),size-vec2<i32>(1));}
    if(edge>1.5) {q=((q%size)+size)%size;}
    let color=textureLoad(source,q,0);var rgb=vec3<f32>(0.0);
    if(color.a>0.0) {rgb=clamp(color.rgb/color.a,vec3<f32>(0.0),vec3<f32>(1.0));}
    if(params.data[2].w>0.5) {rgb=encode_rgb(rgb);}
    if(params.data[0].w<0.5) {rgb*=color.a;}
    return vec4<f32>(rgb,color.a);
}
fn convolution_sample(p:vec2<f32>)->vec4<f32> {
    let q=vec2<i32>(floor(p));let f=fract(p);
    return mix(mix(convolution_load(q),convolution_load(q+vec2<i32>(1,0)),f.x),mix(convolution_load(q+vec2<i32>(0,1)),convolution_load(q+vec2<i32>(1,1)),f.x),f.y);
}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let at=vec2<i32>(p.xy);let src=textureLoad(source,at,0);let config=params.data[0];
    if(config.x<0.5) {return src;}
    if(config.x>5.5 && config.x<6.5) {
        var straight=vec4<f32>(0.0);if(src.a>0.0) {straight=vec4<f32>(src.rgb/src.a,src.a);}
        if(config.y>0.5) {straight=vec4<f32>(encode_rgb(straight.rgb),straight.a);}
        var result=vec4<f32>(transfer_component(straight.r,4u),transfer_component(straight.g,24u),transfer_component(straight.b,44u),transfer_component(straight.a,64u));
        if(config.y>0.5) {result=vec4<f32>(decode_rgb(result.rgb),result.a);}
        return vec4<f32>(result.rgb*result.a,result.a);
    }
    if(config.x>6.5) {
        let order=vec2<i32>(config.yz);let anchor=params.data[1].xy;let unit=params.data[2].xy;
        var sum=vec4<f32>(0.0);
        for(var y=0;y<order.y;y++) {for(var x=0;x<order.x;x++) {
            let position=p.xy-vec2<f32>(0.5)+(vec2<f32>(f32(x),f32(y))-anchor)*unit;
            let index=u32(order.x*order.y-1-y*order.x-x);
            sum+=convolution_sample(position)*scalar(12u+index);
        }}
        let div=params.data[1].z;let bias=params.data[1].w;var alpha=clamp(sum.a/div+bias,0.0,1.0);
        var rgb=vec3<f32>(0.0);
        if(config.w>0.5) {alpha=src.a;rgb=clamp(sum.rgb/div+bias,vec3<f32>(0.0),vec3<f32>(1.0));}
        else if(alpha>0.0) {rgb=clamp(sum.rgb/div+bias*alpha,vec3<f32>(0.0),vec3<f32>(alpha))/alpha;}
        if(params.data[2].w>0.5) {rgb=decode_rgb(rgb);}
        return vec4<f32>(rgb*alpha,alpha);
    }
    if(config.x<1.5) {
        let s=config.y;let horizontal=config.z<0.5;
        let extent=select(textureDimensions(source).y,textureDimensions(source).x,horizontal);
        let radius=i32(min(ceil(4.0*s),f32(extent-1u)));
        let step=select(vec2<i32>(0,1),vec2<i32>(1,0),horizontal);
        var sum=src*(cdf(0.5/s)-cdf(-0.5/s));
        for(var d=1;d<=radius;d++) {
            let weight=cdf((f32(d)+0.5)/s)-cdf((f32(d)-0.5)/s);
            sum+=(load_source(at+step*d)+load_source(at-step*d))*weight;
        }
        return sum;
    }
    if(config.x<2.5) {
        let c=params.data[2];let a=shadow_alpha(p.xy-vec2<f32>(0.5)-params.data[1].xy)*c.a;
        if(config.w>0.5) {return vec4<f32>(c.rgb*a,a);}
        return src+vec4<f32>(c.rgb*a,a)*(1.0-src.a);
    }
    if(config.x>3.5 && config.x<4.5) {return sample_source(p.xy-vec2<f32>(0.5)-params.data[1].xy);}
    if(config.x>4.5) {
        let horizontal=config.z<0.5;let extent=select(textureDimensions(source).y,textureDimensions(source).x,horizontal);
        let radius=i32(min(config.y,f32(extent)));let step=select(vec2<i32>(0,1),vec2<i32>(1,0),horizontal);
        var result=select(vec4<f32>(0.0),vec4<f32>(1.0),config.w>0.5);
        for(var i=-radius;i<=radius;i++) {
            let c=load_source(at+step*i);
            result=select(max(result,c),min(result,c),config.w>0.5);
        }
        return result;
    }
    var straight=vec4<f32>(0.0);
    if(src.a>0.0) {straight=vec4<f32>(src.rgb/src.a,src.a);}
    let out=clamp(vec4<f32>(dot(params.data[3],straight),dot(params.data[4],straight),dot(params.data[5],straight),dot(params.data[6],straight))+params.data[7],vec4<f32>(0.0),vec4<f32>(1.0));
    return vec4<f32>(out.rgb*out.a,out.a);
}
