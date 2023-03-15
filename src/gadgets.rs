// use halo2_proofs::{
//     arithmetic::Field,
//     circuit::{Layouter, SimpleFloorPlanner},
//     plonk::{Circuit, ConstraintSystem, Error},
// };
// use std::marker::PhantomData;

use halo2_proofs::{
    arithmetic::{Field, FieldExt},
    circuit::{Layouter, Value},
    plonk::{Column, ConstraintSystem, Error, Expression, Fixed, VirtualCells},
    poly::Rotation,
};

mod account_update;
mod byte_bit;
mod canonical_representation;
mod is_zero;
mod key_bit;
// mod mpt_update;
// mod one_hot;
mod poseidon;
// mod storage_leaf;
// mod storage_parents;
