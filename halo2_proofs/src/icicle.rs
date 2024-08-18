use group::ff::PrimeField;
use std::sync::Arc;

use icicle_bn254::curve::{CurveCfg, G1Projective, ScalarCfg};

use icicle_cuda_runtime::memory::{DeviceVec, HostSlice};

pub use halo2curves::CurveAffine;
use icicle_core::field::Field;
use icicle_core::{
    curve::{Affine, Curve},
    msm,
};
use icicle_cuda_runtime::memory::HostOrDeviceSlice;
use icicle_cuda_runtime::stream::CudaStream;
use std::{env, mem};

type ScalarField = Field<8, ScalarCfg>;

pub fn should_use_cpu_msm(size: usize) -> bool {
    size <= (1
        << u8::from_str_radix(&env::var("ICICLE_SMALL_K").unwrap_or("8".to_string()), 10).unwrap())
}

fn u32_from_u8(u8_arr: &[u8; 32]) -> [u32; 8] {
    let mut t = [0u32; 8];
    for i in 0..8 {
        t[i] = u32::from_le_bytes([
            u8_arr[4 * i],
            u8_arr[4 * i + 1],
            u8_arr[4 * i + 2],
            u8_arr[4 * i + 3],
        ]);
    }
    return t;
}

fn repr_from_u32<C: CurveAffine>(u32_arr: &[u32; 8]) -> <C as CurveAffine>::Base {
    let t: &[<<C as CurveAffine>::Base as PrimeField>::Repr] =
        unsafe { mem::transmute(&u32_arr[..]) };
    return PrimeField::from_repr(t[0]).unwrap();
}

fn is_infinity_point(point: &G1Projective) -> bool {
    let inf_point = G1Projective::zero();
    inf_point.z.eq(&point.z)
}

fn icicle_scalars_from_c<C: CurveAffine>(coeffs: &[C::Scalar]) -> Vec<ScalarField> {
    let _coeffs = [Arc::new(
        coeffs.iter().map(|x| x.to_repr()).collect::<Vec<_>>(),
    )];

    let _coeffs: &Arc<Vec<[u32; 8]>> = unsafe { mem::transmute(&_coeffs) };
    _coeffs.iter().map(|x| ScalarField::from(*x)).collect::<Vec<_>>()
}

fn icicle_points_from_c<C: CurveAffine>(bases: &[C]) -> Vec<Affine<CurveCfg>> {
    let _bases = [Arc::new(
        bases
            .iter()
            .map(|p| {
                let coordinates = p.coordinates().unwrap();
                [coordinates.x().to_repr(), coordinates.y().to_repr()]
            })
            .collect::<Vec<_>>(),
    )];

    let _bases: &Arc<Vec<[[u8; 32]; 2]>> = unsafe { mem::transmute(&_bases) };
    _bases
        .iter()
        .map(|x| {
            let tx = u32_from_u8(&x[0]);
            let ty = u32_from_u8(&x[1]);

            Affine::<CurveCfg>::from_limbs(tx, ty)
        })
        .collect::<Vec<_>>()
}

fn c_from_icicle_point<C: CurveAffine>(point: &G1Projective) -> C::Curve {
    let (x, y) = if is_infinity_point(point) {
        (
            repr_from_u32::<C>(&[0u32; 8]),
            repr_from_u32::<C>(&[0u32; 8]),
        )
    } else {
        let mut affine: Affine<CurveCfg> = Affine::<CurveCfg>::from(*point);

        (
            repr_from_u32::<C>(&affine.x.into()),
            repr_from_u32::<C>(&affine.y.into()),
        )
    };

    // TODO: Point is not on the curve
    let affine = C::from_xy(x, y);

    return affine.unwrap().to_curve();
}

pub fn multiexp_on_device<C: CurveAffine>(mut coeffs: &[C::Scalar], bases: &[C]) -> C::Curve {
    let binding = icicle_scalars_from_c::<C>(coeffs);
    let coeffs = HostSlice::from_slice(&binding[..]);
    let binding = icicle_points_from_c(bases);
    let bases = HostSlice::from_slice(&binding[..]);

    let mut i = 0;
    // check all c points can be converted to icicle point
    while i < bases.len() {
        let converted_icicle_point = c_from_icicle_point::<C>(&bases[i].to_projective());
        i = i + 1;
    }

    let mut msm_results = DeviceVec::<G1Projective>::cuda_malloc(1).unwrap();
    let mut cfg = msm::MSMConfig::default();
    let stream = CudaStream::create().unwrap();
    cfg.ctx.stream = &stream;
    cfg.is_async = true;
    cfg.large_bucket_factor = 10;
    cfg.c = 16;
    msm::msm(coeffs, bases, &cfg, &mut msm_results[..]).unwrap();
    stream.synchronize().unwrap();

    let mut msm_host_result = vec![G1Projective::zero(); 1];
    msm_results
        .copy_to_host(HostSlice::from_mut_slice(&mut msm_host_result[..]))
        .unwrap();

    println!("msm point: {:?}", msm_host_result);

    let msm_point = c_from_icicle_point::<C>(&msm_host_result[0]);

    println!("msm point: {:?}", msm_point);
    msm_point
}
