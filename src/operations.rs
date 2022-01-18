//! The constraint system matrix for operations inside the arity-2 Merkle Patricia Tree, it would:
//  * constraint hashType transition for both old and new hashtype from the lookup table ☑
//  * constraint old <-> New hashType from the lookup table ☑
//  * assign and constraint IsFirst row according to NewHashType ☑
//  * constraint the root of each first row must be the new root hash of prevs opeartion by inducing
//    a auxing "roots" column ☑
//  * inducing a depth column for accumulating path ☑
//  * constraint path as bit except when newhashtype is leaf ☑
//  * verify the key column by accumulating the path bit and LeafPath bits ☑
//  * (TODO) verify the sibling and oldhash when "leaf extension" hashtype is encountered
//
//  Following is the EXPECTED layout of the chip
//
//  |-----||--------|------------------|------------------|---------|-------|--------|--------|--------|--------|--------|----------------|----------------|
//  | row ||IsFirst*|    OldHashType   |    NewHashType   |  path   |  key  |siblings|OldHash |  hash  | depth**| roots**| TypePairTable**|TypeTransTable**|
//  |-----||--------|------------------|------------------|---------|-------|--------|--------|--------|--------|--------|----------------|--=-------------|
//  |  0  ||  -------------------------------------------------------- padding row --------------------------------------------------------------------    |
//  |  1  ||   1    |       Empty      |      Leaf        | LeafPath|Leafkey|        | rootx  | root0  |   1    | root0  |                |                |
//  |  2  ||   1    |        Mid       |      Mid         | cbit_1  |       |        | root0  | root1  |   1    | root1  |                |                |
//  |  3  ||   0    |      LeafExt     |      Mid         | cbit_2  |       |        |        | hash1  |   2    | root1  |                |                |
//  |  4  ||   0    |   LeafExtFinal   |      Mid         | cbit_3  |       |        |        | hash2  |   4    | root1  |                |                |
//  |  5  ||   0    |       Empty      |      Leaf        | LeafPath|Leafkey|        |        | hash3  |   8    | root1  |                |                |
//  |  6  ||   1    |        Mid       |      Mid         | cbit_4  |       |        | root1  | root2  |   1    | root2  |                |                |
//  |-----||--------|------------------|------------------|---------|-------|--------|--------|--------|--------|--------|----------------|----------------|
//
//  * indicate a "controlled" column (being queried and assigned inside chip)
//  ** indicate a "private" column (a controlled column which is only used in the chip)
//


#![allow(unused_imports)]

use crate::serde::HashType;
use ff::Field;
use halo2::{
    arithmetic::FieldExt,
    circuit::{Cell, Chip, Region, Layouter},
    dev::{MockProver, VerifyFailure},
    plonk::{
        Advice, Assignment, Circuit, Column, ConstraintSystem, Error, Expression, Instance,
        Selector, TableColumn,
    },
    poly::Rotation,
};
use lazy_static::lazy_static;
use std::marker::PhantomData;

pub(crate) struct MPTOpChip<F> {
    config: MPTOpChipConfig,
    _marker: PhantomData<F>,
}

#[derive(Clone, Debug)]
pub(crate) struct MPTOpChipConfig {
    pub is_first: Column<Advice>,

    root_aux: Column<Advice>,
    depth_aux: Column<Advice>,
    key_aux: Column<Advice>,
    type_table: (TableColumn, TableColumn),
    trans_table: (TableColumn, TableColumn),
}

#[derive(Clone, Debug)]
pub(crate) struct Mappings {
    op: Vec<(HashType, HashType)>,
    trans: Vec<(HashType, HashType)>,
}

lazy_static! {
    static ref TYPEMAP: Mappings = {
        Mappings {
            op: vec![
                (HashType::Empty, HashType::Leaf),
                (HashType::Leaf, HashType::Leaf),
                (HashType::Middle, HashType::Middle),
                (HashType::LeafExt, HashType::Middle),
                (HashType::LeafExtFinal, HashType::Middle),
            ],
            trans: vec![
                (HashType::Middle, HashType::Middle),
                (HashType::Middle, HashType::Empty), //insert new leaf under a node
                (HashType::Middle, HashType::Leaf),
                (HashType::Middle, HashType::LeafExt),
                (HashType::Middle, HashType::LeafExtFinal),
                (HashType::LeafExt, HashType::LeafExt),
                (HashType::LeafExt, HashType::LeafExtFinal),
                (HashType::LeafExtFinal, HashType::Leaf),
                (HashType::LeafExtFinal, HashType::Empty),
            ],
        }
    };
}

impl<Fp: FieldExt> Chip<Fp> for MPTOpChip<Fp> {
    type Config = MPTOpChipConfig;
    type Loaded = Mappings;

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &TYPEMAP
    }
}

impl<Fp: FieldExt> MPTOpChip<Fp> {
    ///
    ///  OpChip suppose:
    ///  + the range of col in arguments has been constrainted (like is_leaf is {0, 1})
    ///
    fn configure(
        meta: &mut ConstraintSystem<Fp>,
        s_row: Selector,
        sibling: Column<Advice>,
        path: Column<Advice>,
        old_hash_type: Column<Advice>,
        new_hash_type: Column<Advice>,
        old_hash: Column<Advice>,
        new_hash: Column<Advice>,
    ) -> <Self as Chip<Fp>>::Config {
        let is_first = meta.advice_column();
        let key_aux = meta.advice_column();
        let root_aux = meta.advice_column();
        let depth_aux = meta.advice_column();
        let type_table = (meta.lookup_table_column(), meta.lookup_table_column());
        let trans_table = (meta.lookup_table_column(), meta.lookup_table_column());

        //transition - old
        meta.lookup(|meta| {
            let border =
                Expression::Constant(Fp::one()) - meta.query_advice(is_first, Rotation::cur());
            let hash = border.clone() * meta.query_advice(old_hash_type, Rotation::cur());
            let prev_hash = border * meta.query_advice(old_hash_type, Rotation::prev());

            vec![(prev_hash, trans_table.0), (hash, trans_table.1)]
        });

        //transition - new
        meta.lookup(|meta| {
            let border =
                Expression::Constant(Fp::one()) - meta.query_advice(is_first, Rotation::cur());
            let hash = border.clone() * meta.query_advice(new_hash_type, Rotation::cur());
            let prev_hash = border * meta.query_advice(new_hash_type, Rotation::prev());

            vec![(prev_hash, trans_table.0), (hash, trans_table.1)]
        });

        //old - new
        meta.lookup(|meta| {
            let old_hash = meta.query_advice(old_hash_type, Rotation::cur());
            let new_hash = meta.query_advice(new_hash_type, Rotation::cur());

            vec![(old_hash, type_table.0), (new_hash, type_table.1)]
        });

        //notice we need to enforce the row 0's equality to constraint it as 1
        meta.enable_equality(is_first.into());
        meta.create_gate("is first", |meta| {
            let sel = meta.query_selector(s_row);
            let is_first = meta.query_advice(is_first, Rotation::next());
            let new_hash_type = meta.query_advice(new_hash_type, Rotation::cur());
            let leaf_type = Expression::Constant(Fp::from(HashType::Leaf as u64));

            // is_first.next ∈ {0, 1}
            // if is_leaf is_first.next = 1
            // notice we need extra constraint to set the first row is 1
            // this constraint also enforce the first row of unused region must set is_first to 1
            vec![
                sel.clone()
                    * (Expression::Constant(Fp::one()) - is_first.clone())
                    * is_first.clone(),
                sel * is_first * (new_hash_type - leaf_type),
            ]
        });

        meta.create_gate("path bit", |meta| {
            let sel = meta.query_selector(s_row);
            let new_hash_type = meta.query_advice(new_hash_type, Rotation::cur());
            let leaf_type = Expression::Constant(Fp::from(HashType::Leaf as u64));

            let path = meta.query_advice(path, Rotation::cur());
            let path_bit = (Expression::Constant(Fp::one()) - path.clone()) * path;

            // if (new_hash_type is not leaf) path ∈ {0, 1}
            vec![sel * path_bit * (new_hash_type - leaf_type)]
        });

        meta.create_gate("calc key", |meta| {
            let sel = meta.query_selector(s_row);
            let is_first = meta.query_advice(is_first, Rotation::cur());
            let path_cur = meta.query_advice(path, Rotation::cur())
                * meta.query_advice(depth_aux, Rotation::cur());
            let key_cur = path_cur - meta.query_advice(key_aux, Rotation::cur());

            // if is_first key = path * depth
            // else key = path * depth + key.prev
            vec![
                sel.clone() * is_first.clone() * key_cur.clone(),
                sel * (Expression::Constant(Fp::one()) - is_first)
                    * (meta.query_advice(key_aux, Rotation::prev()) + key_cur),
            ]
        });

        meta.create_gate("root aux", |meta| {
            let sel = meta.query_selector(s_row);
            let is_first = meta.query_advice(is_first, Rotation::cur());
            let root_aux_cur = meta.query_advice(root_aux, Rotation::cur());
            let root_aux_prev = meta.query_advice(root_aux, Rotation::prev());
            let hash = meta.query_advice(new_hash, Rotation::cur());

            // if is_first root_aux == hash
            // else root_aux == root_aux.prev
            vec![
                sel.clone() * is_first.clone() * (root_aux_cur.clone() - hash),
                sel
                    * (Expression::Constant(Fp::one()) - is_first)
                    * (root_aux_cur - root_aux_prev),
            ]
        });

        meta.create_gate("op continue", |meta| {
            let sel = meta.query_selector(s_row);
            let is_first = meta.query_advice(is_first, Rotation::cur());
            let old_hash = meta.query_advice(old_hash, Rotation::cur());
            let root_aux = meta.query_advice(root_aux, Rotation::prev());

            vec![
                sel * is_first * (old_hash - root_aux),
            ]
        });

        meta.create_gate("depth aux", |meta| {
            let sel = meta.query_selector(s_row);
            let is_first = meta.query_advice(is_first, Rotation::cur());
            let depth_aux_cur = meta.query_advice(depth_aux, Rotation::cur());
            let depth_aux_prev = meta.query_advice(depth_aux, Rotation::prev());

            // if is_first depth == 1
            // else depth = depth.prev * 2
            vec![
                sel.clone()
                    * is_first.clone()
                    * (Expression::Constant(Fp::one()) - depth_aux_cur.clone()),
                sel * (Expression::Constant(Fp::one()) - is_first)
                    * (depth_aux_prev * Expression::Constant(Fp::from(2u64)) - depth_aux_cur),
            ]
        });

        //TODO: verify sibling

        MPTOpChipConfig {
            is_first,
            key_aux,
            root_aux,
            depth_aux,
            type_table,
            trans_table,
        }
    }

    // fill region for the first row (head row for padding)
    pub fn fill_heading(
        &self,
        region: &mut Region<'_, Fp>,
        old_hash: Fp,
    ) -> Result<(), Error> {

        region.assign_advice(|| "depth padding",  self.config().key_aux, 0, || Ok(Fp::zero()))?;
        region.assign_advice(|| "key padding", self.config().depth_aux, 0, || Ok(Fp::zero()))?;
        region.assign_advice(|| "root padding", self.config().root_aux, 0, || Ok(old_hash))?;
        //need to pad it as 1 to depress unexpected lookup
        region.assign_advice(|| "is_first padding", self.config().is_first, 0, || Ok(Fp::one()))?;
        //also need to fix the "is_first" flag in first working row
        region.assign_advice_from_constant(|| "top of is_first", self.config().is_first, 1, Fp::one())?;

        Ok(())
    }

    // fill data for a single op in spec position of the region, 
    // notice the first op lay in offset = 1
    // should return how many rows has been filled
    pub fn fill_aux(
        &self,
        region: &mut Region<'_, Fp>,
        offset: usize,
        new_hash_types: &Vec<HashType>,
        hash: &Vec<Fp>,
        path: &Vec<Fp>,
    ) -> Result<usize, Error> {

        assert_eq!(new_hash_types.len(), hash.len());
        assert!(hash.len() > 0, "input must not empty");

        let is_first = self.config().is_first;
        let key_aux = self.config().key_aux;
        let root_aux = self.config().root_aux;
        let depth_aux = self.config().depth_aux;

        let mut cur_root = Fp::zero();
        let mut cur_depth = Fp::zero();
        let mut acc_key = Fp::zero();
        let mut is_first_col = true;        
        //assign rest of is_first according to hashtypes
        for (index, val) in new_hash_types
                            .iter()
                            .zip(hash.iter())
                            .zip(path.iter())
                            .enumerate() 
        {
            let ((hash_type, hash), path) = val;
            let index = index + offset;
            region.assign_advice(
                || "is_first",
                is_first,
                index + 1,
                || {
                    Ok(match *hash_type {
                        HashType::Leaf => Fp::one(),
                        _ => Fp::zero(),
                    })
                },
            )?;

            cur_root = if is_first_col { *hash } else { cur_root };
            cur_depth = if is_first_col { Fp::one() } else { cur_depth.double() };
            acc_key = *path * cur_depth +
                if is_first_col { Fp::zero() } else { acc_key };

            region.assign_advice(|| "root", root_aux, index, || Ok(cur_root))?;

            region.assign_advice(
                || "depth",
                depth_aux,
                index,
                || Ok(cur_depth),
            )?;

            region.assign_advice(
                || "key",
                key_aux,
                index,
                || Ok(acc_key),
            )?;

            is_first_col = match *hash_type {
                HashType::Leaf => true,
                _ => false,
            };
        }

        Ok(hash.len())     
    }

    //fill hashtype table
    pub fn load(
        &self,
        layouter: &mut impl Layouter<Fp>,
    ) -> Result<(), Error> {

        layouter.assign_table(
            || "trans table",
            |mut table| {
                let (cur_col, next_col) = self.config().trans_table;
                for (offset, trans) in self.loaded().trans.iter().enumerate() {
                    let (cur, next) = trans;
                    table.assign_cell(
                        || "cur hash",
                        cur_col,
                        offset,
                        || Ok(Fp::from(*cur as u64)),
                    )?;

                    table.assign_cell(
                        || "next hash",
                        next_col,
                        offset,
                        || Ok(Fp::from(*next as u64)),
                    )?;
                }
                Ok(())
            },
        )?;

        layouter.assign_table(
            || "op table",
            |mut table| {
                let (old_col, new_col) = self.config().type_table;
                for (offset, op) in self.loaded().op.iter().enumerate() {
                    let (old, new) = op;
                    table.assign_cell(
                        || "old hash",
                        old_col,
                        offset,
                        || Ok(Fp::from(*old as u64)),
                    )?;

                    table.assign_cell(
                        || "new hash",
                        new_col,
                        offset,
                        || Ok(Fp::from(*new as u64)),
                    )?;
                }
                Ok(())
            },
        )?;

        Ok(())
    }

    pub fn construct(config: MPTOpChipConfig) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(unused_imports)]

    use super::*;
    use crate::test_utils::*;
    use halo2::{
        circuit::{Cell, SimpleFloorPlanner},
        dev::{MockProver, VerifyFailure},
        plonk::{Circuit, Expression, Selector},
    };

    #[derive(Clone, Debug)]
    struct MPTTestConfig {
        s_row: Selector,
        sibling: Column<Advice>,
        path: Column<Advice>,
        old_hash_type: Column<Advice>,
        new_hash_type: Column<Advice>,
        old_hash: Column<Advice>,
        new_hash: Column<Advice>,
        chip: MPTOpChipConfig,
    }

    #[derive(Clone, Default)]
    struct MPTTestSingleOpCircuit {
        pub old_hash_type: Vec<HashType>,
        pub new_hash_type: Vec<HashType>,
        pub path: Vec<Fp>,
        pub old_hash: Vec<Fp>,
        pub new_hash: Vec<Fp>,
        pub siblings: Vec<Fp>, //siblings from top to bottom
    }

    impl Circuit<Fp> for MPTTestSingleOpCircuit {
        type Config = MPTTestConfig;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
            let s_row = meta.selector();
            let sibling = meta.advice_column();
            let path = meta.advice_column();
            let old_hash_type = meta.advice_column();
            let new_hash_type = meta.advice_column();
            let old_hash = meta.advice_column();
            let new_hash = meta.advice_column();

            let constant = meta.fixed_column();
            meta.enable_constant(constant);

            MPTTestConfig {
                s_row,
                sibling,
                path,
                old_hash_type,
                new_hash_type,
                old_hash,
                new_hash,
                chip: MPTOpChip::configure(
                    meta,
                    s_row,
                    sibling,
                    path,
                    old_hash_type,
                    new_hash_type,
                    old_hash,
                    new_hash,
                ),
            }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), Error> {

            let op_chip = MPTOpChip::<Fp>::construct(config.chip.clone());
            layouter.assign_region(
                || "op main",
                |mut region| {

                    region.assign_advice(
                        || "path padding", 
                        config.path,
                        0,
                        || Ok(Fp::zero()))?;

                    op_chip.fill_heading(&mut region, self.old_hash[0])?;
                    op_chip.fill_aux(&mut region, 1, &self.new_hash_type, &self.new_hash, &self.path)?;
                    self.fill_layer(&config, &mut region, 1)
                },
                
            )?;

            op_chip.load(&mut layouter)?;
            Ok(())
        }
    }


    impl MPTTestSingleOpCircuit {
        pub fn fill_layer(
            &self,
            config: &MPTTestConfig,
            region: &mut Region<'_, Fp>,
            offset: usize,
        ) -> Result<usize, Error> {

            for ind in 0..self.path.len() {
                let offset = offset + ind;
                config.s_row.enable(region, offset)?;

                region.assign_advice(
                    || "path", 
                    config.path,
                    offset,
                    || Ok(self.path[ind]))?;
                region.assign_advice(
                    || "sibling",
                    config.sibling,
                    offset,
                    || Ok(self.siblings[ind]),
                )?;
                region.assign_advice(
                    || "hash_old",
                    config.old_hash,
                    offset,
                    || Ok(self.old_hash[ind]),
                )?;
                region.assign_advice(
                    || "hash_new",
                    config.new_hash,
                    offset,
                    || Ok(self.new_hash[ind]),
                )?;
                region.assign_advice(
                    || "hash_type_old",
                    config.old_hash_type,
                    offset,
                    || Ok(Fp::from(self.old_hash_type[ind] as u64)),
                )?;
                region.assign_advice(
                    || "hash_type_new",
                    config.new_hash_type,
                    offset,
                    || Ok(Fp::from(self.new_hash_type[ind] as u64)),
                )?;
            }

            Ok(self.path.len())
        }
    }

    lazy_static! {

        static ref DEMOCIRCUIT1: MPTTestSingleOpCircuit = {
            MPTTestSingleOpCircuit {
                siblings: vec![Fp::zero()],
                old_hash: vec![Fp::zero()],
                new_hash: vec![Fp::from(11u64)],
                path: vec![Fp::from(4u64)], //the key is 0b100u64
                old_hash_type: vec![HashType::Empty],
                new_hash_type: vec![HashType::Leaf],
            }            
        };

        static ref DEMOCIRCUIT2: MPTTestSingleOpCircuit = {    
            MPTTestSingleOpCircuit {
                siblings: vec![Fp::from(11u64), rand_fp()],
                old_hash: vec![Fp::from(11u64), Fp::zero()],
                new_hash: vec![Fp::from(22u64), rand_fp()],
                path: vec![Fp::one(), Fp::from(8u64)], //the key is 0b10001u64
                old_hash_type: vec![HashType::LeafExtFinal, HashType::Empty],
                new_hash_type: vec![HashType::Middle, HashType::Leaf],
            }            
        };

        static ref DEMOCIRCUIT3: MPTTestSingleOpCircuit = {
            let siblings = vec![Fp::from(11u64), Fp::zero(), Fp::from(22u64), rand_fp()];
            let mut old_hash = vec![Fp::from(22u64)];
            let mut new_hash = vec![Fp::from(33u64)];
            for _ in 0..3 {
                old_hash.push(rand_fp());
                new_hash.push(rand_fp());
            }
    
            MPTTestSingleOpCircuit {
                siblings,
                old_hash,
                new_hash,
                path: vec![Fp::one(), Fp::zero(), Fp::one(), Fp::from(5u64)], //the key is 0b101101u64
                old_hash_type: vec![
                    HashType::Middle,
                    HashType::LeafExt,
                    HashType::LeafExtFinal,
                    HashType::Empty,
                ],
                new_hash_type: vec![
                    HashType::Middle,
                    HashType::Middle,
                    HashType::Middle,
                    HashType::Leaf,
                ],
            }            
        };
    }    


    #[test]
    fn test_single_op() {
        let k = 4;
        let prover = MockProver::<Fp>::run(k, &*DEMOCIRCUIT1, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));           
        let prover = MockProver::<Fp>::run(k, &*DEMOCIRCUIT2, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));        
        let prover = MockProver::<Fp>::run(k, &*DEMOCIRCUIT3, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }

    #[derive(Clone, Default)]
    struct MPTTestOpCircuit {
        pub ops: Vec<MPTTestSingleOpCircuit>,
    }

    impl Circuit<Fp> for MPTTestOpCircuit {
        type Config = MPTTestConfig;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
            MPTTestSingleOpCircuit::configure(meta)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), Error> {

            let op_chip = MPTOpChip::<Fp>::construct(config.chip.clone());

            layouter.assign_region(
                || "multi op main",
                |mut region| {

                    region.assign_advice(
                        || "path padding", 
                        config.path,
                        0,
                        || Ok(Fp::zero()))?;

                    let start_root = self.ops[0].old_hash[0];
                    op_chip.fill_heading(&mut region, start_root)?;

                    let mut offset = 1;
                    for op in self.ops.iter() {

                        op_chip.fill_aux(&mut region, offset, &op.new_hash_type, &op.new_hash, &op.path)?;
                        offset += op.fill_layer(&config, &mut region, offset)?; 
                    }
                    
                    Ok(())
                },
                
            )?;

            op_chip.load(&mut layouter)?;

            Ok(())
        }
    }

    #[test]
    fn test_mutiple_op() {

        let k = 4;

        let circuit = MPTTestOpCircuit {
            ops: vec![DEMOCIRCUIT1.clone(), DEMOCIRCUIT2.clone(), DEMOCIRCUIT3.clone()],
        };

        // Generate layout graph
        
        use plotters::prelude::*;
        let root = BitMapBackend::new("layout.png", (1024, 768)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root
            .titled("Test Circuit Layout", ("sans-serif", 60))
            .unwrap();

        halo2::dev::CircuitLayout::default()
            // You can optionally render only a section of the circuit.
            //.view_width(0..2)
            //.view_height(0..16)
            // You can hide labels, which can be useful with smaller areas.
            .show_labels(true)
            // Render the circuit onto your area!
            // The first argument is the size parameter for the circuit.
            .render(k, &circuit, &root)
            .unwrap();
        

        let prover = MockProver::<Fp>::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));

    }    
    
}
