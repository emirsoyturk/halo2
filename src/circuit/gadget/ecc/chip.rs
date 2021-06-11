use super::EccInstructions;
use crate::circuit::gadget::utilities::{copy, CellValue, Var};
use crate::constants;
use ff::{Field, PrimeFieldBits};
use halo2::{
    arithmetic::CurveAffine,
    circuit::{Chip, Layouter},
    plonk::{Advice, Column, ConstraintSystem, Error, Fixed, Permutation, Selector},
};
use std::marker::PhantomData;

pub(super) mod add;
pub(super) mod add_incomplete;
// pub(super) mod mul;
// pub(super) mod mul_fixed;
pub(super) mod witness_point;
// pub(super) mod witness_scalar_fixed;

/// A curve point represented in affine (x, y) coordinates. Each coordinate is
/// assigned to a cell.
#[derive(Clone, Debug)]
pub struct EccPoint<C: CurveAffine> {
    /// x-coordinate
    x: CellValue<C::Base>,
    /// y-coordinate
    y: CellValue<C::Base>,
}

impl<C: CurveAffine> EccPoint<C> {
    /// Returns the value of this curve point, if known.
    pub fn point(&self) -> Option<C> {
        match (self.x.value(), self.y.value()) {
            (Some(x), Some(y)) => {
                if x == C::Base::zero() && y == C::Base::zero() {
                    Some(C::identity())
                } else {
                    Some(C::from_xy(x, y).unwrap())
                }
            }
            _ => None,
        }
    }
    /// The cell containing the affine short-Weierstrass x-coordinate,
    /// or 0 for the zero point.
    pub fn x(&self) -> CellValue<C::Base> {
        self.x
    }
    /// The cell containing the affine short-Weierstrass y-coordinate,
    /// or 0 for the zero point.
    pub fn y(&self) -> CellValue<C::Base> {
        self.y
    }
}

/// Configuration for the ECC chip
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(non_snake_case)]
pub struct EccConfig<C: CurveAffine> {
    /// Advice columns needed by instructions in the ECC chip.
    pub advices: [Column<Advice>; 10],

    /// Coefficients of interpolation polynomials for x-coordinates (used in fixed-base scalar multiplication)
    pub lagrange_coeffs: [Column<Fixed>; constants::H],
    /// Fixed z such that y + z = u^2 some square, and -y + z is a non-square. (Used in fixed-base scalar multiplication)
    pub fixed_z: Column<Fixed>,

    /// Incomplete addition
    pub q_add_incomplete: Selector,
    /// Complete addition
    pub q_add: Selector,
    /// Variable-base scalar multiplication (hi half)
    pub q_mul_hi: Selector,
    /// Variable-base scalar multiplication (lo half)
    pub q_mul_lo: Selector,
    /// Selector used in scalar decomposition for variable-base scalar mul
    pub q_mul_decompose_var: Selector,
    /// Variable-base scalar multiplication (final scalar)
    pub q_mul_complete: Selector,
    /// Fixed-base full-width scalar multiplication
    pub q_mul_fixed: Selector,
    /// Fixed-base signed short scalar multiplication
    pub q_mul_fixed_short: Selector,
    /// Witness point
    pub q_point: Selector,
    /// Witness full-width scalar for fixed-base scalar mul
    pub q_scalar_fixed: Selector,
    /// Witness signed short scalar for full-width fixed-base scalar mul
    pub q_scalar_fixed_short: Selector,
    /// Permutation
    pub perm: Permutation,
    _marker: PhantomData<C>,
}

/// A chip implementing EccInstructions
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EccChip<C: CurveAffine> {
    config: EccConfig<C>,
}

impl<C: CurveAffine> Chip<C::Base> for EccChip<C> {
    type Config = EccConfig<C>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<C: CurveAffine> EccChip<C>
where
    C::Scalar: PrimeFieldBits,
{
    pub fn construct(config: <Self as Chip<C::Base>>::Config) -> Self {
        Self { config }
    }

    #[allow(non_snake_case)]
    pub fn configure(
        meta: &mut ConstraintSystem<C::Base>,
        advices: [Column<Advice>; 10],
        perm: Permutation,
    ) -> <Self as Chip<C::Base>>::Config {
        let config = EccConfig::<C> {
            advices,
            lagrange_coeffs: [
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
            ],
            fixed_z: meta.fixed_column(),
            q_add_incomplete: meta.selector(),
            q_add: meta.selector(),
            q_mul_hi: meta.selector(),
            q_mul_lo: meta.selector(),
            q_mul_decompose_var: meta.selector(),
            q_mul_complete: meta.selector(),
            q_mul_fixed: meta.selector(),
            q_mul_fixed_short: meta.selector(),
            q_point: meta.selector(),
            q_scalar_fixed: meta.selector(),
            q_scalar_fixed_short: meta.selector(),
            perm,
            _marker: PhantomData,
        };

        // Create witness point gate
        {
            let config: witness_point::Config<C> = (&config).into();
            config.create_gate(meta);
        }

        // Create incomplete point addition gate
        {
            let config: add_incomplete::Config<C> = (&config).into();
            config.create_gate(meta);
        }

        // Create complete point addition gate
        {
            let add_config: add::Config<C> = (&config).into();
            add_config.create_gate(meta);
        }

        config
    }
}

impl<C: CurveAffine> EccInstructions<C> for EccChip<C>
where
    C::Scalar: PrimeFieldBits,
{
    type ScalarFixed = (); // TODO
    type ScalarFixedShort = (); // TODO
    type ScalarVar = (); // TODO
    type Point = EccPoint<C>;
    type X = CellValue<C::Base>;
    type FixedPoints = (); // TODO
    type FixedPointsShort = (); // TODO

    fn witness_scalar_var(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _value: Option<C::Base>,
    ) -> Result<Self::ScalarVar, Error> {
        todo!()
    }

    fn witness_scalar_fixed(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _value: Option<C::Scalar>,
    ) -> Result<Self::ScalarFixed, Error> {
        todo!()
    }

    fn witness_scalar_fixed_short(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _value: Option<C::Scalar>,
    ) -> Result<Self::ScalarFixedShort, Error> {
        todo!()
    }

    fn witness_point(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        value: Option<C>,
    ) -> Result<Self::Point, Error> {
        let config: witness_point::Config<C> = self.config().into();
        layouter.assign_region(
            || "witness point",
            |mut region| config.assign_region(value, 0, &mut region),
        )
    }

    fn extract_p(point: &Self::Point) -> &Self::X {
        &point.x
    }

    fn add_incomplete(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        a: &Self::Point,
        b: &Self::Point,
    ) -> Result<Self::Point, Error> {
        let config: add_incomplete::Config<C> = self.config().into();
        layouter.assign_region(
            || "incomplete point addition",
            |mut region| config.assign_region(a, b, 0, &mut region),
        )
    }

    fn add(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        a: &Self::Point,
        b: &Self::Point,
    ) -> Result<Self::Point, Error> {
        let config: add::Config<C> = self.config().into();
        layouter.assign_region(
            || "complete point addition",
            |mut region| config.assign_region(a, b, 0, &mut region),
        )
    }

    fn mul(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _scalar: &Self::ScalarVar,
        _base: &Self::Point,
    ) -> Result<Self::Point, Error> {
        todo!()
    }

    fn mul_fixed(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _scalar: &Self::ScalarFixed,
        _base: &Self::FixedPoints,
    ) -> Result<Self::Point, Error> {
        todo!()
    }

    fn mul_fixed_short(
        &self,
        _layouter: &mut impl Layouter<C::Base>,
        _scalar: &Self::ScalarFixedShort,
        _base: &Self::FixedPointsShort,
    ) -> Result<Self::Point, Error> {
        todo!()
    }
}
