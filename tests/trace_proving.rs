use halo2_mpt_circuits::{operation::AccountOp, serde, EthTrie};
use halo2_proofs::dev::MockProver;
use halo2_proofs::halo2curves::bn256::{Bn256, Fr as Fp, G1Affine};
use halo2_proofs::plonk::{create_proof, keygen_pk, keygen_vk, verify_proof};
use halo2_proofs::poly::commitment::ParamsProver;
use halo2_proofs::poly::kzg::commitment::{
    KZGCommitmentScheme, ParamsKZG as Params, ParamsVerifierKZG as ParamsVerifier,
};
use halo2_proofs::poly::kzg::multiopen::{ProverSHPLONK, VerifierSHPLONK};
use halo2_proofs::poly::kzg::strategy::SingleStrategy;
use halo2_proofs::transcript::{
    Blake2bRead, Blake2bWrite, Challenge255, PoseidonRead, PoseidonWrite, TranscriptRead,
    TranscriptReadBuffer, TranscriptWriterBuffer,
};
use halo2_proofs::SerdeFormat;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const TEST_TRACE: &str = include_str!("./dual_code_hash/traces_1.json");
const TEST_TRACE_SMALL: &str = include_str!("./dual_code_hash/traces_1.json");
const TEST_TRACE_READONLY: &str = include_str!("./dual_code_hash/traces_1.json");

#[test]
fn trace_read_only() {
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE_READONLY).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    println!("{:?}", ops[0]);

    let k = 8;

    let mut data: EthTrie<Fp> = Default::default();
    data.add_ops(ops);
    let (circuit, _) = data.circuits(200);

    let prover = MockProver::<Fp>::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

#[test]
fn trace_to_eth_trie_each() {
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE_SMALL).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    for op in ops {
        let k = 6;
        println!("{:?}", op);

        let mut data: EthTrie<Fp> = Default::default();
        data.add_op(op);
        let (circuit, _) = data.circuits(40);

        let prover = MockProver::<Fp>::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }
}

#[test]
fn trace_to_eth_trie() {
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE_SMALL).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    let k = 8;

    let mut data: EthTrie<Fp> = Default::default();
    data.add_ops(ops);
    let (circuit, _) = data.circuits(200);

    let prover = MockProver::<Fp>::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

#[test]
fn vk_validity() {
    let params = Params::<Bn256>::unsafe_setup(10);
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE_SMALL).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    let mut data: EthTrie<Fp> = Default::default();
    data.add_ops(ops);
    let (circuit, _) = data.to_circuits((200, None), &[]);

    let vk1 = keygen_vk(&params, &circuit).unwrap();

    let mut vk1_buf: Vec<u8> = Vec::new();
    vk1.write(&mut vk1_buf, SerdeFormat::RawBytesUnchecked)
        .unwrap();

    let data: EthTrie<Fp> = Default::default();
    let (circuit, _) = data.to_circuits((200, None), &[]);
    let vk2 = keygen_vk(&params, &circuit).unwrap();

    let mut vk2_buf: Vec<u8> = Vec::new();
    vk2.write(&mut vk2_buf, SerdeFormat::RawBytesUnchecked)
        .unwrap();

    assert_eq!(vk1_buf, vk2_buf);
}

#[test]
fn proof_and_verify() {
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    let k = 10;

    let params = Params::<Bn256>::unsafe_setup(k);
    let os_rng = ChaCha8Rng::from_seed([101u8; 32]);
    let mut transcript = Blake2bWrite::<_, G1Affine, Challenge255<_>>::init(vec![]);

    let mut data: EthTrie<Fp> = Default::default();
    data.add_ops(ops);
    let (circuit, _) = data.to_circuits((200, None), &[]);

    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    let vk = keygen_vk(&params, &circuit).unwrap();
    let pk = keygen_pk(&params, vk, &circuit).unwrap();

    create_proof::<KZGCommitmentScheme<Bn256>, ProverSHPLONK<'_, Bn256>, _, _, _, _>(
        &params,
        &pk,
        &[circuit],
        &[&[]],
        os_rng,
        &mut transcript,
    )
    .unwrap();

    let proof_script = transcript.finalize();
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(&proof_script[..]);
    let verifier_params: ParamsVerifier<Bn256> = params.verifier_params().clone();
    let strategy = SingleStrategy::new(&params);

    let data: EthTrie<Fp> = Default::default();
    let (circuit, _) = data.to_circuits((200, None), &[]);
    let vk = keygen_vk(&params, &circuit).unwrap();

    verify_proof::<KZGCommitmentScheme<Bn256>, VerifierSHPLONK<'_, Bn256>, _, _, _>(
        &verifier_params,
        &vk,
        strategy,
        &[&[]],
        &mut transcript,
    )
    .unwrap();
}

#[test]
fn circuit_connection() {
    let data: Vec<serde::SMTTrace> = serde_json::from_str(TEST_TRACE).unwrap();
    let ops: Vec<AccountOp<Fp>> = data
        .into_iter()
        .map(|tr| (&tr).try_into().unwrap())
        .collect();

    let k = 13;

    let params = Params::<Bn256>::unsafe_setup(k);
    let os_rng = ChaCha8Rng::from_seed([101u8; 32]);

    let mut data: EthTrie<Fp> = Default::default();
    data.add_ops(ops);

    let (mpt_rows, hash_rows) = data.use_rows();
    println!("mpt {}, hash {}", mpt_rows, hash_rows);

    let commit_indexs = halo2_mpt_circuits::CommitmentIndexs::new::<Fp>();
    let trie_index = commit_indexs.hash_tbl_begin();
    let hash_index = commit_indexs.hash_tbl_begin_at_accompanied_circuit();

    let (trie_circuit, hash_circuit) = data.circuits(200);
    let hash_table_size = [0u8; 5];

    let vk = keygen_vk(&params, &trie_circuit).unwrap();
    let pk = keygen_pk(&params, vk, &trie_circuit).unwrap();

    let mut transcript = PoseidonWrite::<_, G1Affine, Challenge255<_>>::init(vec![]);
    create_proof::<KZGCommitmentScheme<Bn256>, ProverSHPLONK<'_, Bn256>, _, _, _, _>(
        &params,
        &pk,
        &[trie_circuit],
        &[&[]],
        os_rng.clone(),
        &mut transcript,
    )
    .unwrap();
    let proof_script = transcript.finalize();

    let rw_commitment_state = {
        let mut transcript = PoseidonRead::<_, _, Challenge255<G1Affine>>::init(&proof_script[..]);
        (0..trie_index).for_each(|_| {
            transcript.read_point().unwrap();
        });
        hash_table_size.map(|_| transcript.read_point().unwrap())
    };
    //log::info!("rw_commitment_state {:?}", rw_commitment_state);

    let vk = keygen_vk(&params, &hash_circuit).unwrap();
    let pk = keygen_pk(&params, vk, &hash_circuit).unwrap();

    dbg!("");
    let mut transcript = PoseidonWrite::<_, _, Challenge255<_>>::init(vec![]);
    create_proof::<KZGCommitmentScheme<Bn256>, ProverSHPLONK<'_, Bn256>, _, _, _, _>(
        &params,
        &pk,
        &[hash_circuit],
        &[&[]],
        os_rng,
        &mut transcript,
    )
    .unwrap();
    let proof_script = transcript.finalize();

    let rw_commitment_evm = {
        let mut transcript = PoseidonRead::<_, _, Challenge255<G1Affine>>::init(&proof_script[..]);
        (0..hash_index).for_each(|_| {
            transcript.read_point().unwrap();
        });
        hash_table_size.map(|_| transcript.read_point().unwrap())
    };
    //log::info!("rw_commitment_evm {:?}", rw_commitment_evm);

    assert_eq!(rw_commitment_evm, rw_commitment_state);
    //log::info!("Same commitment! Test passes!");
}
