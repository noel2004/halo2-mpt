use crate::{
    constraint_builder::{ConstraintBuilder, SelectorColumn},
    gadgets::{
        byte_bit::ByteBitGadget,
        byte_representation::ByteRepresentationConfig,
        canonical_representation::CanonicalRepresentationConfig,
        key_bit::KeyBitConfig,
        mpt_update::{
            byte_representations, key_bit_lookups, mpt_update_keys, MptUpdateConfig,
            MptUpdateLookup,
        },
        poseidon::PoseidonLookup,
        rlc_randomness::RlcRandomness,
    },
    types::Proof,
};
use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::Layouter,
    halo2curves::bn256::Fr,
    plonk::{Challenge, ConstraintSystem, Error, Expression, VirtualCells},
};

/// Config for MptCircuit
#[derive(Clone)]
pub struct MptCircuitConfig {
    selector: SelectorColumn,
    rlc_randomness: RlcRandomness,
    mpt_update: MptUpdateConfig,
    canonical_representation: CanonicalRepresentationConfig,
    key_bit: KeyBitConfig,
    byte_bit: ByteBitGadget,
    byte_representation: ByteRepresentationConfig,
}

impl MptCircuitConfig {
    pub fn configure(
        cs: &mut ConstraintSystem<Fr>,
        evm_word_challenge: Challenge,
        poseidon: &impl PoseidonLookup,
    ) -> Self {
        let selector = SelectorColumn(cs.fixed_column());
        let rlc_randomness = RlcRandomness(evm_word_challenge);
        let mut cb = ConstraintBuilder::new(selector);

        let byte_bit = ByteBitGadget::configure(cs, &mut cb);
        let byte_representation =
            ByteRepresentationConfig::configure(cs, &mut cb, &byte_bit, &rlc_randomness);
        let canonical_representation =
            CanonicalRepresentationConfig::configure(cs, &mut cb, &byte_bit, &rlc_randomness);
        let key_bit = KeyBitConfig::configure(
            cs,
            &mut cb,
            &canonical_representation,
            &byte_bit,
            &byte_bit,
            &byte_bit,
        );

        let mpt_update = MptUpdateConfig::configure(
            cs,
            &mut cb,
            poseidon,
            &key_bit,
            &byte_representation,
            &byte_representation,
            &rlc_randomness,
            &canonical_representation,
        );

        cb.build(cs);

        Self {
            selector,
            rlc_randomness,
            mpt_update,
            key_bit,
            byte_bit,
            canonical_representation,
            byte_representation,
        }
    }

    pub fn assign(
        &self,
        layouter: &mut impl Layouter<Fr>,
        proofs: &[Proof],
        n_rows: usize,
    ) -> Result<(), Error> {
        let randomness = self.rlc_randomness.value(layouter);
        let (u64s, u128s, frs) = byte_representations(proofs);

        layouter.assign_region(
            || "mpt circuit",
            |mut region| {
                for offset in 1..n_rows {
                    self.selector.enable(&mut region, offset);
                }

                // pad canonical_representation to fixed count
                // notice each input cost 32 rows in canonical_representation, and inside
                // assign one extra input is added
                let mut keys = mpt_update_keys(proofs);
                keys.sort();
                keys.dedup();
                let total_rep_size = n_rows / 32 - 1;
                assert!(
                    total_rep_size >= keys.len(),
                    "no enough space for canonical representation of all keys (need {})",
                    keys.len()
                );

                self.canonical_representation.assign(
                    &mut region,
                    randomness,
                    keys.iter()
                        .chain(std::iter::repeat(&Fr::zero()))
                        .take(total_rep_size),
                );
                self.key_bit.assign(&mut region, &key_bit_lookups(proofs));
                self.byte_bit.assign(&mut region);
                self.byte_representation
                    .assign(&mut region, &u64s, &u128s, &frs, randomness);

                let n_assigned_rows = self.mpt_update.assign(&mut region, proofs, randomness);

                assert!(
                    n_assigned_rows <= n_rows,
                    "mpt circuit requires {n_assigned_rows} rows > limit of {n_rows} rows"
                );

                for offset in 1 + n_assigned_rows..n_rows {
                    self.mpt_update.assign_padding_row(&mut region, offset);
                }

                Ok(())
            },
        )
    }

    pub fn lookup_exprs<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> [Expression<F>; 8] {
        self.mpt_update.lookup().map(|q| q.run(meta))
    }
}
