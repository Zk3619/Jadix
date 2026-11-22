use super::{TransformOrigin, ViewAppearance};

/// Projection of a local z=0 surface after its origin and keyframe transform.
/// The matrix is row-major and remains in logical units, including perspective.
#[derive(Debug, serde::Serialize)]
pub struct SurfaceTransform {
    pub matrix: [f64; 9],
    pub rotation: [f64; 3],
    pub scale: [f64; 2],
    pub translation: [f64; 2],
    pub origin: [f64; 2],
    pub backface: bool,
}

pub fn surface_transform(appearance: &ViewAppearance, size: [f64; 2], motion: [f64; 4]) -> Option<SurfaceTransform> {
    if !appearance.is_valid() || size.into_iter().chain(motion).any(|v| !v.is_finite()) || size.iter().any(|v| *v<0.) { return None; }
    let mut m=appearance.transform.map(f64::from);
    let angle=motion[0].to_radians();let (s,c)=angle.sin_cos();
    let before=m;
    for row in 0..4 {
        m[row]=(before[row]*c+before[4+row]*s)*motion[1];
        m[4+row]=(-before[row]*s+before[4+row]*c)*motion[1];
    }
    m[13]+=size[1]*motion[3];
    let TransformOrigin{fraction,offset,z}=appearance.origin;
    let [ox,oy]=[size[0]*f64::from(fraction[0])+f64::from(offset[0]),size[1]*f64::from(fraction[1])+f64::from(offset[1])];
    let oz=f64::from(z);
    let tx=m[12]-m[0]*ox-m[4]*oy-m[8]*oz;
    let ty=m[13]-m[1]*ox-m[5]*oy-m[9]*oz;
    let tw=m[15]-m[3]*ox-m[7]*oy-m[11]*oz;
    let matrix=[m[0]+ox*m[3],m[4]+ox*m[7],tx+ox*tw,
        m[1]+oy*m[3],m[5]+oy*m[7],ty+oy*tw,m[3],m[7],tw];
    let sx=m[0].hypot(m[1]).hypot(m[2]);
    let mut sy=m[4].hypot(m[5]).hypot(m[6]);
    let determinant=m[0]*(m[5]*m[10]-m[9]*m[6])-m[4]*(m[1]*m[10]-m[9]*m[2])+m[8]*(m[1]*m[6]-m[5]*m[2]);
    // Keep planar mirrors as mirrors, rather than introducing a 180-degree tilt.
    if determinant<0. {sy=-sy;}
    let rotation=[(m[6]/sy).atan2(m[10]/m[8].hypot(m[9]).hypot(m[10])).to_degrees(),
        (-m[2]).atan2(m[0].hypot(m[1])).to_degrees(),m[1].atan2(m[0]).to_degrees()];
    let rotation=rotation.map(|v|if v.is_finite(){v}else{0.});
    let backface=determinant!=0. && (m[0]*m[5]-m[4]*m[1])/determinant<0.;
    if matrix.iter().any(|v|!v.is_finite()){return None;}
    Some(SurfaceTransform{matrix,rotation,scale:[sx,sy],translation:[m[12],m[13]],origin:[ox,oy],backface})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_keeps_shear_perspective_origin_and_backface() {
        let mut view=ViewAppearance::default();
        view.transform[4]=0.5;view.transform[3]=0.001;view.origin.z=10.;
        let projected=surface_transform(&view,[100.,80.],[0.,1.,1.,0.]).unwrap();
        let m=projected.matrix;let w=m[6]*50.+m[7]*40.+m[8];
        assert!(((m[0]*50.+m[1]*40.+m[2])/w-50.).abs()<1e-9);
        assert_eq!(m[1],0.5);assert_eq!(m[6],0.0010000000474974513);
        view=ViewAppearance::default();view.transform[0]=-1.;
        assert!(!surface_transform(&view,[100.,80.],[0.,1.,1.,0.]).unwrap().backface);
        view.transform[10]=-1.;
        assert!(surface_transform(&view,[100.,80.],[0.,1.,1.,0.]).unwrap().backface);
        view.transform=[0.;16];assert!(surface_transform(&view,[0.,0.],[0.,1.,1.,0.]).unwrap().rotation.iter().all(|v|v.is_finite()));
    }
}
