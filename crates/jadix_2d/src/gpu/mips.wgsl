@group(0) @binding(0) var source:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
 let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,y*2.-1.,0.,1.);
}
@fragment fn fs(@builtin(position) position:vec4<f32>)->@location(0) vec4<f32> {
 let size=vec2<f32>(textureDimensions(source));let dst=max(floor(size*0.5),vec2(1.));
 let lo=floor(position.xy)*size/dst;let hi=(floor(position.xy)+vec2(1.))*size/dst;
 let origin=floor(lo);var sum=vec4(0.);
 for(var y=0;y<3;y++){for(var x=0;x<3;x++){
  let p=origin+vec2(f32(x),f32(y));let weight=max(min(p+vec2(1.),hi)-max(p,lo),vec2(0.));
  if(all(p<size)) {sum+=textureLoad(source,vec2<i32>(p),0)*weight.x*weight.y;}
 }}return sum/((hi.x-lo.x)*(hi.y-lo.y));
}
