// Analytic intersection area on the GPU. Input triangles form a non-overlapping fill.
struct AreaIn {
 @location(0) bounds:vec4<f32>, @location(1) range:vec4<u32>,
 @location(2) color:vec4<f32>, @location(3) clip:vec4<f32>,
 @location(4) uv_x:vec4<f32>, @location(5) uv_y:vec4<f32>,
 @location(6) sampling_plan:vec4<f32>,
};
struct AreaOut {
 @builtin(position) position:vec4<f32>,
 @location(0) leaf:vec2<f32>, @location(1) @interpolate(flat) range:vec4<u32>,
 @location(2) @interpolate(flat) color:vec4<f32>, @location(3) @interpolate(flat) clip:vec4<f32>,
 @location(4) uv:vec2<f32>,
 @location(5) @interpolate(flat) image_bounds:vec4<f32>,
 @location(6) @interpolate(flat) image_quality:f32,
 @location(7) @interpolate(flat) sampling_plan:vec4<f32>,
};
@vertex fn vs_area(input:AreaIn,@builtin(vertex_index) i:u32)->AreaOut {
 let corners=array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(1.,1.),vec2(0.,0.),vec2(1.,1.),vec2(0.,1.));
 let p=mix(input.bounds.xy,input.bounds.zw,corners[i]);
 let uv=vec2(dot(input.uv_x.xyz,vec3(p,1.)),dot(input.uv_y.xyz,vec3(p,1.)));
 return AreaOut(vec4(p/g.resolution*vec2(2.,-2.)+vec2(-1.,1.),0.,1.),p,input.range,input.color,input.clip,uv,input.uv_x,input.uv_y.x,input.sampling_plan);
}
fn area_vertex(i:u32)->vec4<f32> {return textureLoad(geometry_data,vec2<i32>(i32(i%textureDimensions(geometry_data).x),i32(i/textureDimensions(geometry_data).x)),0);}
// Green's theorem: integrate the edge's clamped horizontal extent over the
// pixel's Y interval. Equivalent to clipping a triangle, without local arrays
// or dynamic array writes (portable to D3D FXC and WebGL shader translators).
fn edge_mass(a:vec2<f32>,b:vec2<f32>,size:vec2<f32>)->f32 {
 let dy=b.y-a.y;if(dy==0.) {return 0.;}
 let y0=max(min(a.y,b.y),0.);let y1=min(max(a.y,b.y),size.y);
 if(y1<=y0) {return 0.;}
 let slope=(b.x-a.x)/dy;let x0=a.x+(y0-a.y)*slope;let x1=a.x+(y1-a.y)*slope;
 let lo=min(x0,x1);let hi=max(x0,x1);var mean=0.;
 if(lo>=size.x) {mean=size.x;} else if(hi>0.) {
  if(hi-lo<0.0000001) {mean=clamp((lo+hi)*0.5,0.,size.x);} else {
   let p=clamp(lo,0.,size.x);let q=clamp(hi,0.,size.x);
   mean=((q-p)*(q+p)*0.5+size.x*max(hi-size.x,0.))/(hi-lo);
  }
 }
 return sign(dy)*(y1-y0)*mean;
}
fn triangle_pixel(a:vec2<f32>,b:vec2<f32>,c:vec2<f32>,lo:vec2<f32>,hi:vec2<f32>)->f32 {
 if(any(max(max(a,b),c)<=lo)||any(min(min(a,b),c)>=hi)) {return 0.;}
 let size=hi-lo;let aa=a-lo;let bb=b-lo;let cc=c-lo;
 return abs(edge_mass(aa,bb,size)+edge_mass(bb,cc,size)+edge_mass(cc,aa,size));
}
// Exact f32 center tests avoid fixed-point hardware edge quantization. Use
// top/left closed, bottom/right open so shared edges own exactly one sample.
fn sample_edge(a:vec2<f32>,b:vec2<f32>,p:vec2<f32>)->bool {
 let d=b-a;let q=p-a;let cross=d.x*q.y-d.y*q.x;
 return cross>0. || (cross==0. && (d.y<0. || (d.y==0. && d.x>0.)));
}
fn triangle_center(a:vec2<f32>,b:vec2<f32>,c:vec2<f32>,p:vec2<f32>)->f32 {
 let ab=b-a;let ac=c-a;let det=ab.x*ac.y-ab.y*ac.x;
 if(det==0.) {return 0.;}
 if(det<0.) {return select(0.,1.,sample_edge(a,c,p)&&sample_edge(c,b,p)&&sample_edge(b,a,p));}
 return select(0.,1.,sample_edge(a,b,p)&&sample_edge(b,c,p)&&sample_edge(c,a,p));
}
fn area_sample(input:AreaOut)->vec3<f32> {
 let at=floor(input.leaf);let lo=max(at,input.clip.xy);let hi=min(at+vec2(1.),input.clip.zw);
 if(any(hi<=lo)) {return vec3(0.);}
 if(g._pad1>0.5 && (any(input.leaf<input.clip.xy)||any(input.leaf>=input.clip.zw))) {return vec3(0.);}
 var area=0.;var uv=input.uv;var best=0.;
 for(var t=0u;t<input.range.y;t++) {
  let offset=input.range.x+t*3u;let a=area_vertex(offset);let b=area_vertex(offset+1u);let c=area_vertex(offset+2u);
  var mass=0.;if(g._pad1>0.5) {mass=triangle_center(a.xy,b.xy,c.xy,input.leaf);}else{mass=triangle_pixel(a.xy,b.xy,c.xy,lo,hi);}area+=mass;
  if(input.range.z==3u && mass>best) {
   let ab=b.xy-a.xy;let ac=c.xy-a.xy;let p=input.leaf-a.xy;let det=ab.x*ac.y-ab.y*ac.x;
   if(abs(det)>0.000000000001) {let u=(p.x*ac.y-p.y*ac.x)/det;let v=(ab.x*p.y-ab.y*p.x)/det;uv=a.zw+u*(b.zw-a.zw)+v*(c.zw-a.zw);best=mass;}
  }
 }
 return vec3(clamp(area,0.,1.),uv);
}
fn area_input(input:AreaOut,value:vec3<f32>)->VsOut {return VsOut(input.position,input.color,value.x,input.leaf,value.yz,input.range.w);}
// This material contains only vertex colors. Calling generic shade here makes
// FXC compile unreachable gradient/Gaussian branches from uniform fill_kind.
@fragment fn fs_area_solid(input:AreaOut)->@location(0) vec4<f32> {
 let alpha=input.color.a*area_sample(input).x*clip_alpha(input.leaf);
 return vec4(linearize_srgb(input.color.rgb)*alpha,alpha);
}
@fragment fn fs_area_brush(input:AreaOut)->@location(0) vec4<f32> {return brush_shade(area_input(input,area_sample(input)));}
@fragment fn fs_area_pattern(input:AreaOut)->@location(0) vec4<f32> {let v=area_sample(input);return pattern_with_plan(area_input(input,v),input.sampling_plan)*v.x;}
@fragment fn fs_area_image(input:AreaOut)->@location(0) vec4<f32> {
 let v=area_sample(input);return reconstruction_with_plan(v.yz*vec2<f32>(textureDimensions(image_tex)),vec2(0.),input.image_quality,input.image_bounds,input.sampling_plan)*input.color.a*clip_alpha(input.leaf)*v.x;
}
