@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var output:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8,1) fn cs(@builtin(global_invocation_id) id:vec3<u32>) {
 let out_size=textureDimensions(output);if(any(id.xy>=out_size)){return;}
 let size=vec2<f32>(textureDimensions(source));let dst=vec2<f32>(out_size);
 let lo=vec2<f32>(id.xy)*size/dst;let hi=vec2<f32>(id.xy+vec2(1u))*size/dst;let origin=floor(lo);var sum=vec4(0.);
 for(var y=0;y<3;y++){for(var x=0;x<3;x++){
  let p=origin+vec2(f32(x),f32(y));let weight=max(min(p+vec2(1.),hi)-max(p,lo),vec2(0.));
  if(all(p<size)){sum+=textureLoad(source,vec2<i32>(p),0)*weight.x*weight.y;}
 }}textureStore(output,vec2<i32>(id.xy),sum/((hi.x-lo.x)*(hi.y-lo.y)));
}
