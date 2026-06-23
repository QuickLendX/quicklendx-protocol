#![cfg(test)]

use super::*;
use crate::invariants::validate_solvency_invariant;
use crate::settlement::MAX_FACE_VALUE;

#[test]
fn test_solvency_invariant_basic() {
    validate_solvency_invariant(10_000, 4_000, 100, 50);
    validate_solvency_invariant(10_000, 9_000, 200, 100);
}

#[test]
fn test_solvency_invariant_boundary() {
    // funded == face (edge case)
    validate_solvency_invariant(10_000, 10_000, 100, 0);
}

#[test]
fn test_solvency_invariant_randomized() {
    for i in 1..20 {
        let face = 10_000 + i * 100;
        let funded = face / 2;

        validate_solvency_invariant(face, funded, 100, 50);
    }
}