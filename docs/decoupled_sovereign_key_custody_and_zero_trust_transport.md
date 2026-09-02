# Decoupling Key Custody: An On-Premises Sovereign Credential Enclave & Zero-Trust Transport Architecture for Enterprise Bitcoin Banking

> **Architectural Whitepaper & Security Specification**  
> *Author:* `@bootlace-dev`  
> *Target Stack:* `GaloyMoney/galoy`, `GaloyMoney/cala`, `GaloyMoney/bria`  
> *Classification:* Public / Open-Source Systems Architecture  
> *Compliance Mapping:* FIPS 140-3 / 140-2 Level 3, NIST SP 800-57, PCI-DSS v4.0.1 (Req 1.3 & 1.4), OCC Bulletin 2020-10, SOC 2 Type II, SLSA Level 3+

---

## ⚡ TL;DR (Executive Summary)

* **The Problem**: Tier-1 commercial banks, credit unions, and sovereign financial institutions cannot adopt cloud Bitcoin banking SaaS (`GaloyMoney/galoy`, `bria`, `cala`) if root private keys or HSM credentials must be hosted in the cloud.
* **The Solution**: A **4-tier decoupled architecture** where cloud orchestration constructs unsigned PSBT transactions while **100% of signing keys remain inside customer on-premises datacenters**:
  1. **Zero-Key Cloud Construction**: Cloud orchestrator (`bria`/`cala`) manages UTXOs and batches without holding any private keys.
  2. **Outbound-Only Reverse gRPC Streams**: On-prem bank enclaves pull PSBT jobs over persistent outbound connections, requiring **0 inbound firewall ports** (PCI-DSS v4.0.1 compliance).
  3. **Enclave Policy Engine**: On-prem daemon validates change descriptor derivations, fee-rate caps, and witness prevouts before releasing HSM sign requests.
  4. **Hardware-Sealed Signing**: FIPS 140-3 / 140-2 Level 3 physical HSMs with BIP-340 / RFC 6979 synthetic nonces and OpenTimestamps Merkle transparency logging.

---

## 1. Executive Summary: The Enterprise Sales Dealbreaker

When scaling Bitcoin banking infrastructure (`GaloyMoney/galoy`, `bria`, `cala`) to tier-1 commercial banks, credit unions, and sovereign financial institutions (e.g. in El Salvador, Africa, or Latin America), **cloud key custody is an immediate regulatory and security blocker**:

* **The Problem**: Banking regulations, sovereign wealth charters, and institutional risk policies strictly forbid third-party cloud SaaS providers from holding root private keys, HSM credentials, or Bitcoin signing keys.
* **The Solution**: **Decoupled Key Custody & Signing Architecture**. Galoy’s cloud/managed software handles high-throughput double-entry ledgers, mobile user APIs, and transaction assembly, while **100% of private keys, HSMs, and signing credentials remain physically anchored inside the customer's on-premises datacenter**.

---

## 2. The 4-Tier Decoupled Enclave Architecture

```
┌───────────────────────────────────────────────┐
│              GALOY MANAGED CLOUD              │
│  - cala (Double-Entry Multi-Currency Ledger)  │
│  - bria (Batching & UTXO Allocation Engine)   │
│  - Constructs Unsigned PSBT Job Queue         │
└───────────────────────▲───────────────────────┘
                        │
                        │ Outbound-Initiated Long-Poll / gRPC Stream
                        │ (0 Inbound Firewall Ports on Bank Datacenter)
                        │
┌───────────────────────┴───────────────────────┐
│          CUSTOMER ON-PREM DATACENTER          │
│  ┌─────────────────────────────────────────┐  │
│  │ 1. Outbound Zero-Trust Worker Gateway   │  │
│  │    - Strict IP/CIDR Egress Whitelist    │  │
│  │    - SLSA L3+ Binary Provenance Checks  │  │
│  └────────────────────┬────────────────────┘  │
│                       ▼                       │
│  ┌─────────────────────────────────────────┐  │
│  │ 2. Bank Policy Engine & PSBT Inspector  │  │
│  │    - Deterministic Change Validation    │  │
│  │    - Absolute Fee & Fee-Rate Caps       │  │
│  │    - Idempotency WAL & RBF State Machine│  │
│  └────────────────────┬────────────────────┘  │
│                       ▼                       │
│  ┌─────────────────────────────────────────┐  │
│  │ 3. Active-Passive Dual HSM Enclave      │  │
│  │    - FIPS 140-3 (Thales) / 140-2 (Yubi) │  │
│  │    - Non-Exportable PKCS#11 Native Keys │  │
│  │    - BIP-340 / RFC 6979 Synthetic Nonce │  │
│  │    - OpenTimestamps Anchored Merkle Log │  │
│  └─────────────────────────────────────────┘  │
└───────────────────────────────────────────────┘
```

### Tier 1: Cloud-Side Zero-Key Transaction Construction
* `bria` and `cala` run in the cloud or managed Kubernetes cluster.
* When an on-chain withdrawal, batch settlement, or cold rebalance occurs, `bria` executes coin selection and constructs an **unsigned Partially Signed Bitcoin Transaction (PSBT)**.
* **The Zero-Key Guarantee**: The cloud holds **zero private keys**. Even in the catastrophic event of a full cloud root compromise, an adversary cannot steal funds or sign transactions.

### Tier 2: Outbound-Only Zero-Trust Worker Gateway (PCI-DSS v4.0.1 Compliant)
* **Zero Inbound Firewall Ports**: Enterprise bank datacenters strictly prohibit inbound cloud connections (PCI-DSS v4.0.1 Requirements 1.3 & 1.4 / OCC guidelines). The on-prem signing daemon initiates an **outbound-only persistent gRPC stream / reverse WireGuard tunnel** to pull pending PSBT jobs.
* **Strict Egress Lockdown**: Kernel-level firewall whitelists **strictly hard-coded destination IP/CIDR ranges** (zero runtime DNS resolution) to eliminate DNS-tunnel exfiltration vectors.
* **SLSA Level 3+ Binary Provenance**: Enclave signing daemons require cryptographic attestation (Sigstore Cosign / in-toto) verified against measured hardware root-of-trust before execution.

### Tier 3: The On-Premises Policy Engine & PSBT Inspector

The bank's local signing daemon does not blindly execute incoming requests. It enforces strict local risk verification:

1. **Deterministic Change Address Validation**: Independently derives and verifies all change output script pubkeys against the bank's master internal descriptors (`m/84'/0'/0'/1/*`, `m/86'/0'/0'/1/*`), preventing change-hijacking attacks.
2. **Fee-Rate & Absolute Fee Caps**: Asserts transaction fee rate ($\text{sat/vB}$) and total fee percentage do not exceed strict policy thresholds, preventing fee-siphoning attacks.
3. **Witness UTXO Prevout Assertion**: Validates complete previous output data across all inputs to prevent blind-signing sighash malleability.
4. **Automated Velocity & Sanctions Filtering**: Automatically signs transactions below configured thresholds (e.g. $< \$1,000$) while checking destination scripts against sanctioned routing tables.
5. **M-of-N Executive Approval Gates**: High-value transactions (e.g. $> \$100,000$) require $M$-of-$N$ executive hardware FIDO2 / WebAuthn token sign-offs directly on hardware keys before unlocking HSM signing sessions.
6. **Local Idempotency WAL & RBF State Machine**: Records `BatchId` + `ReplacementSequence` + $\text{SHA-256}(\text{PSBT})$ locally, enabling safe BIP-125 Replace-By-Fee (RBF) fee-bumps while guaranteeing absolute double-signing prevention during in-flight network partitions or power outages.

### Tier 4: Hardware-Sealed Key Signing (Physical Enclave)
* **Dedicated Physical HSM Demarcation**:
  * **Chassis / PCIe HSMs (FIPS 140-3 Level 3)**: Enterprise tier (**Thales Luna PCIe**, **Securosys Primus**) with active physical zeroization and environmental failure protection circuitry.
  * **Compact Embedded Tokens (FIPS 140-2 Level 3 Physical)**: Sovereign tier (**YubiHSM 2 FIPS**) with passive tamper-evident potting.
  * Root Bitcoin keys are generated directly within the physical module boundary and are **strictly non-exportable**; signing requests execute entirely within hardware via PKCS#11 with native multi-authorization (or vendor extensions for BIP-340 Schnorr on `secp256k1`).
* **Tamper-Evident Merkle Transparency Log with Public Anchoring**:
  * All signing operations stream to an append-only Merkle transparency log (RFC 6962 model) on immutable WORM storage.
  * To defeat split-view / equivocation attacks, Signed Tree Heads (STHs) are periodically anchored to the Bitcoin blockchain via **OpenTimestamps** and multi-party witness cosigning.

---

## 3. High-Assurance OpSec & Cryptographic Invariants (High-Assurance Defensive Invariants)

Enterprise security collapses when teams rely on default operating system configurations. This architecture enforces four non-negotiable defensive invariants:

### A. The Elimination of the Global `ca-certificates` Package (Trust-Surface Minimization)
* **The Flaw**: Standard Linux distributions ship with $>150$ pre-installed public Certificate Authorities (commercial vendors and foreign nation-states). Any compromised root CA can issue fraudulent certificates to silently intercept banking traffic.
* **The Invariant**:
  * **Physically purge the `ca-certificates` package** from the on-prem signing container.
  * The appliance trusts **strictly a single, pinned institutional Root CA public key**.
  * All communications require **mTLS client certificates** and SPKI fingerprint matching, making public CA spoofing mathematically impossible.

### B. Hypervisor Entropy Invariance & Canonical BIP-340 / RFC 6979 Synthetic Nonce Derivation
* **The Flaw**: Virtualized on-prem bank VMs (VMware ESXi, KVM) frequently suffer from entropy pool depletion due to lack of physical hardware interrupts, risking predictable ECDSA/Schnorr $k$-nonces and private key loss.
* **The Invariant**:
  * Mandatory physical hardware TRNG / TPM 2.0 (`tpm_rng`) / `virtio-rng` injection into `/dev/urandom`.
  * **Canonical BIP-340 Nonce Derivation**: All Schnorr signatures enforce exact BIP-340 tagged-hash synthetic nonces:
    $$t = \text{bytes}(d') \oplus \text{hash}_{\text{BIP0340/aux}}(a)$$
    $$\text{rand} = \text{hash}_{\text{BIP0340/nonce}}\left(t \mathbin{\Vert} \text{bytes}_x(P) \mathbin{\Vert} m\right)$$
    $$(k = \text{int}(\text{rand}) \pmod n)$$
    and ECDSA signatures enforce RFC 6979 HMAC-DRBG, **mathematically guaranteeing zero private key leakage even under total hypervisor TRNG failure**.

### C. The 3-Tier Zero-Trust Transport Matrix & HPKE AAD Armor
1. **Tier 1 (Point-to-Point Tunnel Overlay)**: Zero CA dependency; uses strictly static Ed25519 public keys pinned in `known_hosts` (OpenSSH) or static X25519 peer public keys in `wg0.conf` (WireGuard) to provide absolute immunity against BGP route hijacks and DNS cache poisoning.
2. **Tier 2 (SPKI-Pinned mTLS)**: SHA-256 SPKI leaf fingerprint pinning guarantees instant connection termination if a certificate deviates from the expected public key.
3. **Tier 3 (Application-Layer HPKE Encryption & AAD Armor)**: Financial payloads are encrypted using **Hybrid Public Key Encryption (HPKE, RFC 9180)** with `DHKEM(X25519, HKDF-SHA256)` and `ChaCha20-Poly1305` AEAD. The monotonic sequence number, batch ID, tenant ID, and UTC timestamp are cryptographically bound into the **Authenticated Additional Data (AAD)** header, guaranteeing absolute immunity against in-window replay and out-of-order manipulation.
