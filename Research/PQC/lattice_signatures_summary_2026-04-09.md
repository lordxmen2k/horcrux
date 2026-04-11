# Lattice-Based Digital Signatures - Research Summary

**Date:** April 9, 2026  
**Sources:** arXiv, PQC-forum, NIST PQC Standardization Materials

---

## Executive Summary

Lattice-based digital signatures continue to be the dominant approach in post-quantum cryptography (PQC). The field has seen significant standardization progress with NIST finalizing FIPS 204 (ML-DSA) in August 2024, and ongoing research into optimization, alternative schemes, and practical deployment considerations.

---

## Key Standardized Schemes

### 1. ML-DSA (CRYSTALS-Dilithium) - FIPS 204
- **Status:** NIST standard finalized August 2024
- **Foundation:** Module-LWE and Module-LWR problems
- **Approach:** Fiat-Shamir with aborts paradigm
- **Strengths:** Good performance, simple implementation, moderate key/signature sizes
- **Variants:** ML-DSA-44, ML-DSA-65, ML-DSA-87 (different security levels)

### 2. FN-DSA (FALCON) - FIPS 206 (In Development)
- **Foundation:** NTRU problem with GPV hash-and-sign framework
- **Approach:** Fast Fourier sampling over structured lattices
- **Status:** Draft standard under development as of March 2026
- **Characteristics:** Shorter signatures than ML-DSA but more complex implementation

### 3. SLH-DSA (SPHINCS+) - FIPS 205
- **Status:** NIST standard finalized August 2024
- **Foundation:** Hash functions only (conservative security)
- **Role:** Non-lattice backup providing cryptographic diversity
- **Trade-off:** Larger signatures (7,856–17,088 bytes) but minimal security assumptions

---

## Emerging Lattice-Based Signatures (NIST Onramp Process)

### HAWK
- **Status:** Selected for Round 2 of NIST Additional Signatures process (January 2025)
- **Type:** Lattice-based hash-and-sign signature scheme
- **Relation to Falcon:** Similar structure using Gram matrix for bad basis
- **Potential:** May offer performance improvements over existing lattice schemes

### HAETAE
- **Approach:** Shorter Fiat-Shamir signatures with strong security guarantees
- **Target:** Improved efficiency and signature size compared to Dilithium

### Recent Research Papers (2026)

1. **"Performance Analysis of Quantum-Secure Digital Signature Algorithms in Blockchain"** (arXiv:2601.17785)
   - Comparative analysis of ML-DSA, Falcon, Hawk, and HAETAE
   - Focus on blockchain deployment scenarios
   - Examines trade-offs in key sizes, signature sizes, and computational cost

2. **"Lattice: A Post-Quantum Settlement Layer"** (arXiv:2603.07947)
   - Proposes ML-DSA-44 exclusively for blockchain settlement
   - No classical fallback (pure post-quantum approach)
   - Addresses perpetual security through tail emission

3. **"ZK-ACE: Identity-Centric Zero-Knowledge Authorization"** (arXiv:2603.07974)
   - Challenges of verifying lattice-based signatures in ZK circuits
   - ML-DSA verification requires millions of R1CS constraints
   - Highlights complexity of non-native field arithmetic for lattice schemes

---

## Performance Characteristics

| Scheme | Public Key (bytes) | Private Key (bytes) | Signature (bytes) | Security Level |
|--------|-------------------|---------------------|-------------------|----------------|
| ML-DSA-44 | 1,312 | 2,528 | 2,420 | NIST Category 2 |
| ML-DSA-87 | 2,592 | 4,864 | 4,595 | NIST Category 5 |
| FALCON-512 | ~1,000 | ~2,000 | ~666 | NIST Category 1 |
| SLH-DSA | ~32 | ~64 | 7,856-17,088 | Varies |

---

## Implementation Considerations

### Number Theoretic Transform (NTT)
- Critical operation in lattice-based signatures
- Discrete analogue of FFT for polynomial multiplication
- Performance bottleneck in many implementations

### ARM Processor Optimizations
- Ongoing research into ARM-specific optimizations
- Mobile and embedded deployment requires careful optimization
- Polynomial arithmetic and NTT operations benefit from NEON instructions

### Side-Channel Protection
- Lattice schemes require careful implementation to prevent timing attacks
- Constant-time implementations essential for security
- Hardware acceleration being explored (RISC-V Vector Cryptography Extension)

---

## Standardization Timeline

- **August 2024:** FIPS 203 (ML-KEM), FIPS 204 (ML-DSA), FIPS 205 (SLH-DSA) published
- **March 2025:** HQC selected for standardization
- **2026:** Third round of Additional Digital Signature schemes planned
- **September 2025:** Sixth NIST PQC Standardization Conference (tentative)

---

## Key Challenges

1. **Signature Size:** All PQC signatures significantly larger than ECDSA/RSA
2. **Verification Cost:** Lattice operations computationally intensive
3. **ZK Circuit Integration:** Verifying lattice signatures in ZK proofs is expensive
4. **Deployment Complexity:** Migration from classical cryptography ongoing

---

## Future Directions

- Continued optimization for constrained devices
- Hybrid classical/PQC approaches during transition period
- Exploration of non-lattice alternatives (hash-based, code-based, multivariate)
- Hardware acceleration and dedicated PQC instructions

---

## Sources

- NIST PQC Standardization Process: https://csrc.nist.gov/projects/post-quantum-cryptography
- arXiv Cryptography and Security (cs.CR)
- PQC Forum (Google Groups)
- NIST IR 8547: Transition to PQC Standards

---

*Generated: April 9, 2026*
