//! Sinsemilla generators
use group::Curve;
use halo2::arithmetic::{CurveAffine, CurveExt};

/// Number of bits of each message piece in $\mathsf{SinsemillaHashToPoint}$
pub const K: usize = 10;

/// The largest integer such that $2^c \leq (r_P - 1) / 2$, where $r_P$ is the order
/// of Pallas.
pub const C: usize = 253;

// Sinsemilla Q generators

/// SWU hash-to-curve personalization for Sinsemilla $Q$ generators.
pub const Q_PERSONALIZATION: &str = "z.cash:SinsemillaQ";

/// Generator used in SinsemillaHashToPoint for note commitment
pub const Q_NOTE_COMMITMENT_M_GENERATOR: ([u8; 32], [u8; 32]) = (
    [
        93, 116, 168, 64, 9, 186, 14, 50, 42, 221, 70, 253, 90, 15, 150, 197, 93, 237, 176, 121,
        180, 242, 159, 247, 13, 205, 251, 86, 160, 7, 128, 23,
    ],
    [
        99, 172, 73, 115, 90, 10, 39, 135, 158, 94, 219, 129, 136, 18, 34, 136, 44, 201, 244, 110,
        217, 194, 190, 78, 131, 112, 198, 138, 147, 88, 160, 50,
    ],
);

/// Generator used in SinsemillaHashToPoint for IVK commitment
pub const Q_COMMIT_IVK_M_GENERATOR: ([u8; 32], [u8; 32]) = (
    [
        242, 130, 15, 121, 146, 47, 203, 107, 50, 162, 40, 81, 36, 204, 27, 66, 250, 65, 162, 90,
        184, 129, 204, 125, 17, 200, 169, 74, 241, 12, 188, 5,
    ],
    [
        190, 222, 173, 207, 206, 229, 90, 190, 241, 165, 109, 201, 29, 53, 196, 70, 75, 5, 222, 32,
        70, 7, 89, 239, 230, 190, 26, 212, 246, 76, 1, 27,
    ],
);

/// Generator used in SinsemillaHashToPoint for Merkle collision-resistant hash
pub const Q_MERKLE_CRH: ([u8; 32], [u8; 32]) = (
    [
        160, 198, 41, 127, 249, 199, 185, 248, 112, 16, 141, 192, 85, 185, 190, 201, 153, 14, 137,
        239, 90, 54, 15, 160, 185, 24, 168, 99, 150, 210, 22, 22,
    ],
    [
        98, 234, 242, 37, 206, 174, 233, 134, 150, 21, 116, 5, 234, 150, 28, 226, 121, 89, 163, 79,
        62, 242, 196, 45, 153, 32, 175, 227, 163, 66, 134, 53,
    ],
);

// Sinsemilla S generators

/// SWU hash-to-curve personalization for Sinsemilla $S$ generators.
pub const S_PERSONALIZATION: &str = "z.cash:SinsemillaS";

/// Creates the Sinsemilla S generators used in each round of the Sinsemilla hash
pub fn sinsemilla_s_generators<C: CurveAffine>() -> impl Iterator<Item = (C::Base, C::Base)> {
    let hasher = C::CurveExt::hash_to_curve(S_PERSONALIZATION);
    (0..(1 << K)).map(move |j| {
        let point = hasher(&(j as u32).to_le_bytes())
            .to_affine()
            .coordinates()
            .unwrap();
        (*point.x(), *point.y())
    })
}

#[cfg(test)]
mod tests {
    use super::super::{CommitDomain, HashDomain};
    use super::*;
    use crate::constants::{
        COMMIT_IVK_PERSONALIZATION, MERKLE_CRH_PERSONALIZATION, NOTE_COMMITMENT_PERSONALIZATION,
    };
    use group::Curve;
    use halo2::arithmetic::FieldExt;
    use halo2::pasta::pallas;

    #[test]
    fn sinsemilla_s() {
        use super::super::sinsemilla_s::SINSEMILLA_S;
        let mut sinsemilla_s = sinsemilla_s_generators::<pallas::Affine>();
        for s in SINSEMILLA_S.iter() {
            assert_eq!(sinsemilla_s.next().unwrap(), (s.0, s.1));
        }
    }

    #[test]
    fn q_note_commitment_m() {
        let domain = CommitDomain::new(NOTE_COMMITMENT_PERSONALIZATION);
        let point = domain.M.Q;
        let coords = point.to_affine().coordinates().unwrap();

        assert_eq!(
            *coords.x(),
            pallas::Base::from_bytes(&Q_NOTE_COMMITMENT_M_GENERATOR.0).unwrap()
        );
        assert_eq!(
            *coords.y(),
            pallas::Base::from_bytes(&Q_NOTE_COMMITMENT_M_GENERATOR.1).unwrap()
        );
    }

    #[test]
    fn q_commit_ivk_m() {
        let domain = CommitDomain::new(COMMIT_IVK_PERSONALIZATION);
        let point = domain.M.Q;
        let coords = point.to_affine().coordinates().unwrap();

        assert_eq!(
            *coords.x(),
            pallas::Base::from_bytes(&Q_COMMIT_IVK_M_GENERATOR.0).unwrap()
        );
        assert_eq!(
            *coords.y(),
            pallas::Base::from_bytes(&Q_COMMIT_IVK_M_GENERATOR.1).unwrap()
        );
    }

    #[test]
    fn q_merkle_crh() {
        let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);
        let point = domain.Q;
        let coords = point.to_affine().coordinates().unwrap();

        assert_eq!(
            *coords.x(),
            pallas::Base::from_bytes(&Q_MERKLE_CRH.0).unwrap()
        );
        assert_eq!(
            *coords.y(),
            pallas::Base::from_bytes(&Q_MERKLE_CRH.1).unwrap()
        );
    }
}
