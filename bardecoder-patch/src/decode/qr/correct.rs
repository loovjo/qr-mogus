use super::galois::{EXP8, GF8};
use super::BlockInfo;

use crate::util::qr::QRError;

use std::ops::{Div, Mul, Sub};

pub fn correct(block: Vec<u8>, block_info: &BlockInfo) -> Result<Vec<u8>, QRError> {
    correct_with_error_count(block, block_info).map(|r| r.0)
}

pub fn correct_with_error_count(
    mut block: Vec<u8>,
    block_info: &BlockInfo,
) -> Result<(Vec<u8>, u32), QRError> {
    let (all_fine, syndromes) = calculate_syndromes(&block, block_info);

    if all_fine {
        // all fine, nothing to do
        debug!("ALL SYNDROMES WERE ZERO, NO CORRECTION NEEDED");
        return Ok((block, 0));
    }

    let locs = find_locs(block_info, &syndromes)?;

    let distance = calculate_distances(&syndromes, &locs);
    let distance = distance.ok_or(QRError {
        msg: String::from("Could not calculate error distances"),
    })?;

    let mut error_count = 0;

    for i in 0..locs.len() {
        debug!(
            "FIXING LOCATION {} FROM {:08b} TO {:08b}",
            block_info.total_per as usize - 1 - locs[i] as usize,
            block[block_info.total_per as usize - 1 - locs[i] as usize],
            block[block_info.total_per as usize - 1 - locs[i] as usize] ^ distance[i].0
        );

        error_count += distance[i].0.count_ones();
        block[block_info.total_per as usize - 1 - locs[i] as usize] ^= distance[i].0;
    }

    if syndrome(&block, EXP8[0]) != GF8(0) {
        return Err(QRError {
            msg: String::from("Error correcting did not fix corrupted data"),
        });
    }

    Ok((block, error_count))
}

fn calculate_syndromes(block: &[u8], block_info: &BlockInfo) -> (bool, Vec<GF8>) {
    let mut syndromes = vec![GF8(0); (block_info.ec_cap * 2) as usize];

    let mut all_fine = true;
    for i in 0..block_info.ec_cap * 2 {
        syndromes[i as usize] = syndrome(block, EXP8[i as usize]);
        if syndromes[i as usize] != GF8(0) {
            all_fine = false;
        }
    }

    (all_fine, syndromes)
}

fn syndrome(block: &[u8], base: GF8) -> GF8 {
    let mut synd = GF8(0);
    let mut alpha = GF8(1);

    for codeword in block.iter().rev() {
        synd = synd + (alpha * GF8(*codeword));

        alpha = alpha * base;
    }

    synd
}

fn find_locs(block_info: &BlockInfo, syndromes: &[GF8]) -> Result<Vec<usize>, QRError> {
    let z = block_info.ec_cap as usize;
    let mut eq = vec![vec![GF8(0); z + 1]; z];
    for i in 0..z {
        eq[i][..=z].clone_from_slice(&syndromes[i..(z + 1 + i)]);
    }

    let sigma = solve(eq, GF8(0), GF8(1), false);

    let sigma = sigma.ok_or(QRError {
        msg: String::from("Could not calculate SIGMA"),
    })?;

    let mut locs = vec![];

    for (i, exp) in EXP8.iter().enumerate().take(block_info.total_per as usize) {
        let mut x = *exp;
        let mut check_value = sigma[0];
        for s in sigma.iter().skip(1) {
            check_value = check_value + x * *s;
            x = x * *exp;
        }
        check_value = check_value + x;

        if check_value == GF8(0) {
            debug!("LOC {:?} {} ", exp, i);
            locs.push(i);
        }
    }

    debug!("LOCS {:?}", locs);

    Ok(locs)
}

fn calculate_distances(syndromes: &[GF8], locs: &[usize]) -> Option<Vec<GF8>> {
    let mut eq = vec![vec![GF8(0); locs.len() + 1]; locs.len()];
    for i in 0..locs.len() {
        for j in 0..locs.len() {
            eq[i][j] = EXP8[(i * locs[j] as usize) % 255];
        }

        eq[i][locs.len()] = syndromes[i];
    }

    solve(eq, GF8(0), GF8(1), false)
}

fn solve<T>(mut eq: Vec<Vec<T>>, zero: T, one: T, fail_on_rank: bool) -> Option<Vec<T>>
where
    T: Div<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy + PartialEq,
{
    let num_eq = eq.len() as usize;
    if num_eq == 0 {
        return None;
    }

    let num_coeff = eq[0].len();
    if num_coeff == 0 {
        return None;
    }

    for i in 0..num_eq {
        // normalise equation
        for j in (i..num_coeff).rev() {
            // divide all coefficients by the first nonzero
            // the first nonzero will now be GF8(1)
            eq[i][j] = eq[i][j] / eq[i][i];
        }

        // subtract normalised equation from others, multiplied by first coefficient
        // so the coefficients corresponding to the GF8(1) above will be GF8(0)
        for j in i + 1..num_eq {
            for k in (i..num_coeff).rev() {
                eq[j][k] = eq[j][k] - (eq[j][i] * eq[i][k]);
            }
        }

        // If the rank is too low, can't solve
        if fail_on_rank && eq[i][num_coeff - 1] == one {
            return None;
        }
    }

    let mut solution = vec![zero; num_eq];

    for i in (0..num_eq).rev() {
        solution[i] = eq[i][num_coeff - 1];
        for j in i + 1..num_coeff - 1 {
            solution[i] = solution[i] - (eq[i][j] * solution[j]);
        }
    }

    Some(solution)
}
