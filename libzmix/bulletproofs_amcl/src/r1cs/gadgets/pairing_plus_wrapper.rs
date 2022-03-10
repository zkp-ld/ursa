use super::bound_check::{gen_proof_of_bounded_num, verify_proof_of_bounded_num};
use crate::r1cs::R1CSProof;
use crate::utils::get_generators;
use amcl_wrapper::group_elem_g1::{G1Vector, G1};
use amcl_wrapper::{
    field_elem::FieldElement,
    group_elem::GroupElement,
    ECCurve::{big::BIG, ecp::ECP},
};
use ff_zeroize::PrimeField;
use pairing_plus::{
    bls12_381::{Fq, FqRepr, Fr, FrRepr, G1 as PpG1},
    CurveAffine, CurveProjective,
};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::fmt;

#[derive(Debug, Clone)]
pub enum GenRangeProofError {
    ValOverflow,
    InvalidProof,
    InvalidCommitment,
}

impl fmt::Display for GenRangeProofError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                GenRangeProofError::ValOverflow => "val should be integer between 0 and 2^32",
                GenRangeProofError::InvalidProof => "invalid proof",
                GenRangeProofError::InvalidCommitment => "invalid commitment",
            }
        )
    }
}

#[derive(Debug, Clone)]
pub enum VerifyRangeProofError {
    VerificationError,
    InvalidCommitment,
}

impl fmt::Display for VerifyRangeProofError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                VerifyRangeProofError::VerificationError => "verification error of bulletproofs",
                VerifyRangeProofError::InvalidCommitment => "invalid commitment",
            }
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bulletproof {
    proof: R1CSProof,
    commitments: [G1; 2],
}

#[allow(non_snake_case)]
pub fn gen_rangeproof(
    val: &Fr,
    blinding: &Fr,
    lower: u64,
    upper: u64,
    transcript_label: &'static [u8],
    g: &PpG1,
    h: &PpG1,
    c: &PpG1,
) -> Result<Bulletproof, GenRangeProofError> {
    // TODO: should be given as global parameters or issuer-specific public keys
    let G: G1Vector = get_generators("G", 128).into();
    let H: G1Vector = get_generators("H", 128).into();

    let max_bits_in_val: usize = (64 - (upper - lower).leading_zeros()).try_into().unwrap();

    let val_repr = val.into_repr();
    let val_ref = val_repr.as_ref();
    if val_ref[1] > 0 || val_ref[2] > 0 || val_ref[3] > 0 {
        return Err(GenRangeProofError::ValOverflow);
    }
    let val = val_ref[0];

    let blinding = pp_fr_to_amcl_fieldelement(blinding);
    let g = pp_g1_to_amcl_g1(g);
    let h = pp_g1_to_amcl_g1(h);

    // given commitment
    let c = pp_g1_to_amcl_g1(c);

    match gen_proof_of_bounded_num(
        val,
        Some(blinding),
        lower,
        upper,
        max_bits_in_val,
        transcript_label,
        &g,
        &h,
        &G,
        &H,
    ) {
        Ok((proof, (com_v, [com_min, com_max]))) => {
            // check the equality of two commitments generated by bbs+ and bulletproofs
            if c == com_v {
                Ok(Bulletproof {
                    proof,
                    commitments: [com_min, com_max],
                })
            } else {
                Err(GenRangeProofError::InvalidCommitment)
            }
        }
        _ => Err(GenRangeProofError::InvalidProof),
    }
}

#[allow(non_snake_case)]
pub fn verify_rangeproof(
    bp: Bulletproof,
    lower: u64,
    upper: u64,
    transcript_label: &'static [u8],
    g: &PpG1,
    h: &PpG1,
    c: &PpG1,
) -> Result<(), VerifyRangeProofError> {
    // TODO: should be given as global parameters or issuer-specific public keys
    let G: G1Vector = get_generators("G", 128).into();
    let H: G1Vector = get_generators("H", 128).into();

    let max_bits_in_val: usize = (64 - (upper - lower).leading_zeros()).try_into().unwrap();

    let g = pp_g1_to_amcl_g1(g);
    let h = pp_g1_to_amcl_g1(h);

    // given commitment
    let c = pp_g1_to_amcl_g1(c);
    let commitments = &(c, bp.commitments);

    match verify_proof_of_bounded_num(
        lower,
        upper,
        max_bits_in_val,
        bp.proof,
        commitments,
        transcript_label,
        &g,
        &h,
        &G,
        &H,
    ) {
        Ok(_) => Ok(()),
        Err(_) => Err(VerifyRangeProofError::VerificationError),
    }
}

pub fn pp_fr_to_amcl_fieldelement(fr: &Fr) -> FieldElement {
    let frrepr: FrRepr = fr.into_repr();
    let u64_array: &[u64] = frrepr.as_ref();
    let mut bytes: [u8; 48] = [0; 48];
    for i in 0..4 {
        let tmp = u64_array[3 - i].to_be_bytes();
        for j in 0..8 {
            bytes[i * 8 + j + 16] = tmp[j];
        }
    }
    FieldElement::from_bytes(&bytes).unwrap()
}

fn pp_fq_to_amcl_big(fq: Fq) -> BIG {
    let pp_fqrepr: FqRepr = FqRepr::from(fq);
    let pp_u64_array: &[u64] = pp_fqrepr.as_ref();
    let mut bytes: [u8; 48] = [0; 48];
    for i in 0..6 {
        let tmp = pp_u64_array[5 - i].to_be_bytes();
        for j in 0..8 {
            bytes[i * 8 + j] = tmp[j];
        }
    }
    BIG::frombytes(&bytes)
}

pub fn pp_g1_to_amcl_ecp(g1: &PpG1) -> ECP {
    let affine = g1.into_affine();
    let tuple_affine = affine.as_tuple();
    let big_x = pp_fq_to_amcl_big(*tuple_affine.0);
    let big_y = pp_fq_to_amcl_big(*tuple_affine.1);
    ECP::new_bigs(&big_x, &big_y)
}

pub fn pp_g1_to_amcl_g1(g1: &PpG1) -> G1 {
    let ecp = pp_g1_to_amcl_ecp(g1);
    let mut bytes: [u8; 97] = [0; 97];
    ecp.tobytes(&mut bytes, false);
    G1::from_bytes(&bytes).unwrap()
}