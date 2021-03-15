use std::iter;
use std::marker::PhantomData;

use halo2::arithmetic::FieldExt;

pub(crate) mod grain;
pub(crate) mod mds;

mod nullifier;
pub use nullifier::OrchardNullifier;

use grain::SboxType;

/// A specification for a Poseidon permutation.
pub trait Spec<F: FieldExt> {
    /// The type used to hold permutation state, or equivalent-length constant values.
    ///
    /// This must be an array of length [`Spec::width`], that defaults to all-zeroes.
    type State: Default + AsRef<[F]> + AsMut<[F]>;

    /// The type used to hold duplex sponge state.
    ///
    /// This must be an array of length equal to the rate of the duplex sponge (allowing
    /// for a capacity consistent with this specification's security level), that defaults
    /// to `[None; RATE]`.
    type Rate: Default + AsRef<[Option<F>]> + AsMut<[Option<F>]>;

    /// The width of this specification.
    fn width() -> usize;

    /// The number of full rounds for this specification.
    ///
    /// This must be an even number.
    fn full_rounds() -> usize;

    /// The number of partial rounds for this specification.
    fn partial_rounds() -> usize;

    /// The S-box for this specification.
    fn sbox(val: F) -> F;

    /// Side-loaded index of the first correct and secure MDS that will be generated by
    /// the reference implementation.
    ///
    /// This is used by the default implementation of [`Spec::constants`]. If you are
    /// hard-coding the constants, you may leave this unimplemented.
    fn secure_mds(&self) -> usize;

    /// Generates `(round_constants, mds, mds^-1)` corresponding to this specification.
    fn constants(&self) -> (Vec<Self::State>, Vec<Self::State>, Vec<Self::State>) {
        let t = Self::width();
        let r_f = Self::full_rounds();
        let r_p = Self::partial_rounds();

        let mut grain = grain::Grain::new(SboxType::Pow, t as u16, r_f as u16, r_p as u16);

        let round_constants = (0..(r_f + r_p))
            .map(|_| {
                let mut rc_row = Self::State::default();
                for (rc, value) in rc_row
                    .as_mut()
                    .iter_mut()
                    .zip((0..t).map(|_| grain.next_field_element()))
                {
                    *rc = value;
                }
                rc_row
            })
            .collect();

        let (mds, mds_inv) = mds::generate_mds(&mut grain, t, self.secure_mds());

        (
            round_constants,
            mds.into_iter()
                .map(|row| {
                    let mut mds_row = Self::State::default();
                    for (entry, value) in mds_row.as_mut().iter_mut().zip(row.into_iter()) {
                        *entry = value;
                    }
                    mds_row
                })
                .collect(),
            mds_inv
                .into_iter()
                .map(|row| {
                    let mut mds_row = Self::State::default();
                    for (entry, value) in mds_row.as_mut().iter_mut().zip(row.into_iter()) {
                        *entry = value;
                    }
                    mds_row
                })
                .collect(),
        )
    }
}

/// Runs the Poseidon permutation on the given state.
fn permute<F: FieldExt, S: Spec<F>>(
    state: &mut S::State,
    mds: &[S::State],
    round_constants: &[S::State],
) {
    let r_f = S::full_rounds() / 2;
    let r_p = S::partial_rounds();

    let apply_mds = |state: &mut S::State| {
        let mut new_state = S::State::default();
        // Matrix multiplication
        #[allow(clippy::needless_range_loop)]
        for i in 0..S::width() {
            for j in 0..S::width() {
                new_state.as_mut()[i] += mds[i].as_ref()[j] * state.as_ref()[j];
            }
        }
        *state = new_state;
    };

    let full_round = |state: &mut S::State, rcs: &S::State| {
        for (word, rc) in state.as_mut().iter_mut().zip(rcs.as_ref().iter()) {
            *word = S::sbox(*word + rc);
        }
        apply_mds(state);
    };

    let part_round = |state: &mut S::State, rcs: &S::State| {
        for (word, rc) in state.as_mut().iter_mut().zip(rcs.as_ref().iter()) {
            *word += rc;
        }
        // In a partial round, the S-box is only applied to the first state word.
        state.as_mut()[0] = S::sbox(state.as_ref()[0]);
        apply_mds(state);
    };

    iter::empty()
        .chain(iter::repeat(&full_round as &dyn Fn(&mut S::State, &S::State)).take(r_f))
        .chain(iter::repeat(&part_round as &dyn Fn(&mut S::State, &S::State)).take(r_p))
        .chain(iter::repeat(&full_round as &dyn Fn(&mut S::State, &S::State)).take(r_f))
        .zip(round_constants.iter())
        .fold(state, |state, (round, rcs)| {
            round(state, rcs);
            state
        });
}

fn poseidon_duplex<F: FieldExt, S: Spec<F>>(
    state: &mut S::State,
    input: &S::Rate,
    pad_and_add: &dyn Fn(&mut S::State, &S::Rate),
    mds_matrix: &[S::State],
    round_constants: &[S::State],
) -> S::Rate {
    pad_and_add(state, input);

    permute::<F, S>(state, mds_matrix, round_constants);

    let mut output = S::Rate::default();
    for (word, value) in output.as_mut().iter_mut().zip(state.as_ref().iter()) {
        *word = Some(*value);
    }
    output
}

enum SpongeState<F: FieldExt, S: Spec<F>> {
    Absorbing(S::Rate),
    Squeezing(S::Rate),
}

impl<F: FieldExt, S: Spec<F>> SpongeState<F, S> {
    fn absorb(val: F) -> Self {
        let mut input = S::Rate::default();
        input.as_mut()[0] = Some(val);
        SpongeState::Absorbing(input)
    }
}

/// A Poseidon duplex sponge.
pub struct Duplex<F: FieldExt, S: Spec<F>> {
    sponge: SpongeState<F, S>,
    state: S::State,
    pad_and_add: Box<dyn Fn(&mut S::State, &S::Rate)>,
    mds_matrix: Vec<S::State>,
    round_constants: Vec<S::State>,
    _marker: PhantomData<S>,
}

impl<F: FieldExt, S: Spec<F>> Duplex<F, S> {
    /// Constructs a new duplex sponge for the given Poseidon specification.
    pub fn new(
        spec: S,
        initial_capacity_element: F,
        pad_and_add: Box<dyn Fn(&mut S::State, &S::Rate)>,
    ) -> Self {
        let (round_constants, mds_matrix, _) = spec.constants();

        let input = S::Rate::default();
        let mut state = S::State::default();
        state.as_mut()[input.as_ref().len()] = initial_capacity_element;

        Duplex {
            sponge: SpongeState::Absorbing(input),
            state,
            pad_and_add,
            mds_matrix,
            round_constants,
            _marker: PhantomData::default(),
        }
    }

    /// Absorbs an element into the sponge.
    pub fn absorb(&mut self, value: F) {
        match self.sponge {
            SpongeState::Absorbing(ref mut input) => {
                for entry in input.as_mut().iter_mut() {
                    if entry.is_none() {
                        *entry = Some(value);
                        return;
                    }
                }

                // We've already absorbed as many elements as we can
                let _ = poseidon_duplex::<F, S>(
                    &mut self.state,
                    &input,
                    &self.pad_and_add,
                    &self.mds_matrix,
                    &self.round_constants,
                );
                self.sponge = SpongeState::absorb(value);
            }
            SpongeState::Squeezing(_) => {
                // Drop the remaining output elements
                self.sponge = SpongeState::absorb(value);
            }
        }
    }

    /// Squeezes an element from the sponge.
    pub fn squeeze(&mut self) -> F {
        loop {
            match self.sponge {
                SpongeState::Absorbing(ref input) => {
                    self.sponge = SpongeState::Squeezing(poseidon_duplex::<F, S>(
                        &mut self.state,
                        &input,
                        &self.pad_and_add,
                        &self.mds_matrix,
                        &self.round_constants,
                    ));
                }
                SpongeState::Squeezing(ref mut output) => {
                    for entry in output.as_mut().iter_mut() {
                        if let Some(e) = entry.take() {
                            return e;
                        }
                    }

                    // We've already squeezed out all available elements
                    self.sponge = SpongeState::Absorbing(S::Rate::default());
                }
            }
        }
    }
}

/// A domain in which a Poseidon hash function is being used.
pub trait Domain<F: FieldExt, S: Spec<F>>: Copy {
    /// The initial capacity element, encoding this domain.
    fn initial_capacity_element(&self) -> F;

    /// Returns a function that will update the given state with the given input to a
    /// duplex permutation round, applying padding according to this domain specification.
    fn pad_and_add(&self) -> Box<dyn Fn(&mut S::State, &S::Rate)>;
}

/// A Poseidon hash function used with constant input length.
///
/// Domain specified in section 4.2 of https://eprint.iacr.org/2019/458.pdf
#[derive(Clone, Copy, Debug)]
pub struct ConstantLength(pub usize);

impl<F: FieldExt, S: Spec<F>> Domain<F, S> for ConstantLength {
    fn initial_capacity_element(&self) -> F {
        // Capacity value is $length \cdot 2^64 + (o-1)$ where o is the output length.
        // We hard-code an output length of 1.
        F::from_u128((self.0 as u128) << 64)
    }

    fn pad_and_add(&self) -> Box<dyn Fn(&mut S::State, &S::Rate)> {
        Box::new(|state, input| {
            // `Iterator::zip` short-circuits when one iterator completes, so this will only
            // mutate the rate portion of the state.
            for (word, value) in state.as_mut().iter_mut().zip(input.as_ref().iter()) {
                // For constant-input-length hashing, padding consists of the field
                // elements being zero, so we don't add anything to the state.
                if let Some(value) = value {
                    *word += value;
                }
            }
        })
    }
}

/// A Poseidon hash function, built around a duplex sponge.
pub struct Hash<F: FieldExt, S: Spec<F>, D: Domain<F, S>> {
    duplex: Duplex<F, S>,
    domain: D,
}

impl<F: FieldExt, S: Spec<F>, D: Domain<F, S>> Hash<F, S, D> {
    /// Initializes a new hasher.
    pub fn init(spec: S, domain: D) -> Self {
        Hash {
            duplex: Duplex::new(
                spec,
                domain.initial_capacity_element(),
                domain.pad_and_add(),
            ),
            domain,
        }
    }
}

impl<F: FieldExt, S: Spec<F>> Hash<F, S, ConstantLength> {
    /// Hashes the given input.
    ///
    /// # Panics
    ///
    /// Panics if the message length is not the correct length.
    pub fn hash(mut self, message: impl Iterator<Item = F>) -> F {
        let mut length = 0;
        for (i, value) in message.enumerate() {
            length = i + 1;
            self.duplex.absorb(value);
        }
        assert_eq!(length, self.domain.0);
        self.duplex.squeeze()
    }
}

#[cfg(test)]
mod tests {
    use halo2::arithmetic::FieldExt;
    use pasta_curves::pallas;

    use super::{permute, ConstantLength, Hash, OrchardNullifier, Spec};

    #[test]
    fn orchard_spec_equivalence() {
        let message = [pallas::Base::from_u64(6), pallas::Base::from_u64(42)];

        let (round_constants, mds, _) = OrchardNullifier.constants();

        let hasher = Hash::init(OrchardNullifier, ConstantLength(2));
        let result = hasher.hash(message.iter().cloned());

        // The result should be equivalent to just directly applying the permutation and
        // taking the first state element as the output.
        let mut state = [message[0], message[1], pallas::Base::from_u128(2 << 64)];
        permute::<_, OrchardNullifier>(&mut state, &mds, &round_constants);
        assert_eq!(state[0], result);
    }
}
