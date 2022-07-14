// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress, IntoSuiAddress};
use crate::committee::{Committee, EpochId};
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::Base64;
use crate::sui_serde::SuiBitmap;
use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use narwhal_crypto::bls12381::{BLS12381KeyPair, BLS12381Signature, BLS12381PublicKey, BLS12381PublicKeyBytes};
use narwhal_crypto::ed25519::{Ed25519KeyPair, Ed25519Signature, Ed25519PublicKey, Ed25519PublicKeyBytes};
pub use narwhal_crypto::traits::KeyPair as NarwhalKeypair;
pub use narwhal_crypto::traits::{
    AggregateAuthenticator, Authenticator, SigningKey, ToFromBytes, VerifyingKey, VerifyingKeyBytes,
};
use narwhal_crypto::Verifier;
use rand::rngs::OsRng;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha3::Sha3_256;
use signature::Signature as NativeSignature;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use serde_with::Bytes;

// Comment the one you want to use
pub type AuthorityKeyPair = BLS12381KeyPair; // Associated Types don't work here yet for some reason.
pub type AccountKeyPair = Ed25519KeyPair; // Associated Types don't work here yet for some reason.

pub type AuthorityPrivateKey = <AuthorityKeyPair as NarwhalKeypair>::PrivKey;
pub type AuthorityPublicKey = <AuthorityKeyPair as NarwhalKeypair>::PubKey;
pub type AuthorityPublicKeyBytes = <AuthorityPublicKey as VerifyingKey>::Bytes;

// Signatures for Authorities
pub type AuthoritySignature = <<AuthorityKeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig;
pub type AggregateAuthoritySignature =
    <<<AuthorityKeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig as Authenticator>::AggregateSig;



// pub enum AccountPublicKey {
//    Ed25519(Ed25519PrivateKey),
// }

// pub enum AccountSignature {
// 
// }

pub type AccountPrivateKey = <AccountKeyPair as NarwhalKeypair>::PrivKey;
pub type AccountPublicKey = <AccountKeyPair as NarwhalKeypair>::PubKey;
pub type AccountPublicKeyBytes = <AccountPublicKey as VerifyingKey>::Bytes;    

// Signatures for Users
pub type AccountSignature = <<AccountKeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig;
pub type AggregateAccountSignature =
    <<<AccountKeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig as Authenticator>::AggregateSig;

pub trait SuiAuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Self>) -> Self
    where
        T: Signable<Vec<u8>>;
    fn verify<T>(&self, value: &T, author: AuthorityPublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>;
}

impl SuiAuthoritySignature for AuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Self>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    fn verify<T>(&self, value: &T, author: AuthorityPublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: AuthorityPublicKey = author
            .try_into()
            .map_err(|_| SuiError::InvalidAddress)?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        // perform cryptographic signature check
        public_key
            .verify(&message, &self)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }
}

pub fn random_key_pairs<K: NarwhalKeypair>(num: usize) -> Vec<K> {
    let mut items = num;
    let mut rng = OsRng;

    std::iter::from_fn(|| {
        if items == 0 {
            None
        } else {
            items -= 1;
            Some(get_key_pair_from_rng(&mut rng).1)
        }
    })
    .collect::<Vec<_>>()
}

// TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
// TODO: rename to random_key_pair
pub fn get_key_pair<K: NarwhalKeypair>() -> (SuiAddress, K) {
    get_key_pair_from_rng(&mut OsRng)
}

/// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
pub fn get_key_pair_from_rng<K: NarwhalKeypair, R>(csprng: &mut R) -> (SuiAddress, K)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let kp = K::generate(csprng);
    (kp.public_key_bytes().into_sui_address(), kp)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes<K: NarwhalKeypair>(bytes: &[u8]) -> SuiResult<(SuiAddress, K)> {
    let kp = K::generate_from_bytes(bytes).map_err(|e| SuiError::InvalidKeypair {
        error: e.to_string(),
    })?;
    Ok((kp.public_key_bytes().into_sui_address(), kp))
}

// 
// Account Signatures
// 

// Enums for Signatures
const FLAG_LENGTH: usize = 2;

#[derive(Clone, Serialize, Deserialize)]
pub enum Signature {
    Ed25519(Ed25519SuiSignature),
    Empty
}

// Can refactor this with a library
impl Signature {
    pub fn verify<T>(&self, value: &T, author: SuiAddress) -> SuiResult<()> 
        where T: Signable<Vec<u8>>,
    {
        match self {
            Self::Ed25519(sig) => sig.verify(value, author),
            Self::Empty => Err(SuiError::InvalidSignature {
                error: "Empty signature".to_string(),
            })
        }
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        match self {
            Self::Ed25519(sig) => sig.public_key_bytes(),
            Self::Empty => &[]
        }
    }

    pub fn flag_bytes(&self) -> &[u8] {
        match self {
            Self::Ed25519(sig) => sig.flag_bytes(),
            Self::Empty => &[]
        }
    }

    pub fn signature_bytes(&self) -> &[u8] {
        match self {
            Self::Ed25519(sig) => sig.signature_bytes(),
            Self::Empty => &[]
        }
    }

    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Signature>) -> Signature 
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        self.as_ref()
    }
}

impl signature::Signature for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        match bytes.get(0..2).ok_or(signature::Error::new())? {
            x if x == &Ed25519SuiSignature::flag[..] => Ok(Signature::Ed25519(Ed25519SuiSignature::from_bytes(bytes).map_err(|_| signature::Error::new())?)),
            _ => Err(signature::Error::new()),
        }
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(self.signature_bytes());
        let p = base64ct::Base64::encode_string(self.public_key_bytes());
        write!(f, "{s}@{p}")?;
        Ok(())
    }
}

// 
// Ed25519 Sui Signature port
// 
#[serde_as]
#[derive(Clone, Serialize, Deserialize)]
pub struct Ed25519SuiSignature (
    #[serde_as(as = "Bytes")]
    [u8; Self::LENGTH]
);

impl SuiSignature for Ed25519SuiSignature {
    type Sig = Ed25519Signature; 
    type PubKey = Ed25519PublicKey;
    type PubKeyBytes = Ed25519PublicKeyBytes;
    const LENGTH: usize = Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + FLAG_LENGTH;
    const flag: [u8; FLAG_LENGTH] = [0xed, 0x25];

    fn bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    fn from_bytes(bytes: &[u8]) -> SuiResult<Self> {
        if bytes.len() != Self::LENGTH {
            return Err(SuiError::InvalidSignature {
                error: format!("Invalid signature length: {}", bytes.len()),
            });
        }
        let mut result_bytes = [0u8; Self::LENGTH];
        result_bytes.copy_from_slice(bytes);
        return Ok(Ed25519SuiSignature(result_bytes));
    }
}

impl signature::Signer<Signature> for Ed25519KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let signature_bytes: <<Ed25519KeyPair as NarwhalKeypair>::PrivKey as SigningKey>::Sig =
            self.try_sign(msg)?;

        let pk_bytes = self.public_key_bytes();
        let public_key_bytes = pk_bytes.as_ref();
        let mut result_bytes = [0u8; Ed25519SuiSignature::LENGTH];

        result_bytes[..FLAG_LENGTH].copy_from_slice(&Ed25519SuiSignature::flag);
        result_bytes[FLAG_LENGTH..<Self as NarwhalKeypair>::Sig::LENGTH + FLAG_LENGTH].copy_from_slice(&signature_bytes.as_ref());
        result_bytes[<Self as NarwhalKeypair>::Sig::LENGTH + FLAG_LENGTH..].copy_from_slice(public_key_bytes);
        Ok(Signature::Ed25519(Ed25519SuiSignature(result_bytes)))
    }
}

// 
// SuiSignature
// 
trait SuiSignature: Sized {
    type Sig: Authenticator;
    type PubKey: VerifyingKey<Sig = Self::Sig>;
    type PubKeyBytes: VerifyingKeyBytes<PubKey = Self::PubKey>;
    const flag: [u8; FLAG_LENGTH];
    const LENGTH: usize;

    fn bytes(&self) -> &[u8];

    fn flag_bytes(&self) -> &[u8] {
        &self.bytes()[..FLAG_LENGTH]
    }

    fn signature_bytes(&self) -> &[u8] {
        &self.bytes()[FLAG_LENGTH..Self::Sig::LENGTH + FLAG_LENGTH]
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.bytes()[FLAG_LENGTH + Self::Sig::LENGTH..]
    }

    /// This performs signature verification on the passed-in signature, additionally checking
    /// that the signature was performed with a PublicKey belonging to an expected author, indicated by its Sui Address
    fn verify<T>(&self, value: &T, author: SuiAddress) -> SuiResult<()>
    where
        T: Signable<Vec<u8>>,
    {
        let (message, signature, public_key_bytes) = self.get_verification_inputs(value, author)?;

        // is this a cryptographically correct public key?
        // TODO: perform stricter key validation, sp. small order points, see https://github.com/MystenLabs/sui/issues/101
        let public_key = Self::PubKey::from_bytes(public_key_bytes.as_ref())
            .map_err(|err| SuiError::InvalidSignature {
                error: err.to_string(),
            })?;

        // perform cryptographic signature check
        public_key
            .verify(&message, &signature)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }

    fn get_verification_inputs<T>(
        &self,
        value: &T,
        author: SuiAddress,
    ) -> SuiResult<(Vec<u8>, Self::Sig, Self::PubKeyBytes)>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes = Self::PubKeyBytes::from_bytes(self.public_key_bytes())
            .expect("byte lengths match");

        let received_addr = public_key_bytes.into_sui_address();
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!("Signature get_verification_inputs() failure. Author is {author}, received address is {received_addr}")
            });
        }

        // deserialize the signature
        let signature = Self::Sig::from_bytes(self.signature_bytes()).map_err(|err| {
            SuiError::InvalidSignature {
                error: err.to_string(),
            }
        })?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        Ok((message, signature, public_key_bytes))
    }

    fn from_bytes(bytes: &[u8]) -> SuiResult<Self>;
}

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instanted on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}
impl AuthoritySignInfoTrait for AuthoritySignInfo {}

impl Hash for AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature, because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
    }
}

impl AuthoritySignInfo {
    pub fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation<AggregateAuthoritySignature>,
        message_index: usize,
    ) -> SuiResult<()> {
        obligation
            .public_keys
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .push(committee.public_key(&self.authority)?);
        obligation
            .signatures
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .add_signature(self.signature.clone())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid Signature".to_string(),
            })?;
        Ok(())
    }
}

/// Represents at least a quorum (could be more) of authority signatures.
/// STRONG_THRESHOLD indicates whether to use the quorum threshold for quorum check.
/// When STRONG_THRESHOLD is true, the quorum is valid when the total stake is
/// at least the quorum threshold (2f+1) of the committee; when STRONG_THRESHOLD is false,
/// the quorum is valid when the total stake is at least the validity threshold (f+1) of
/// the committee.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityQuorumSignInfo<const STRONG_THRESHOLD: bool> {
    pub epoch: EpochId,
    #[schemars(with = "Base64")]
    pub signature: AggregateAuthoritySignature,
    #[schemars(with = "Base64")]
    #[serde_as(as = "SuiBitmap")]
    pub signers_map: RoaringBitmap,
}

pub type AuthorityStrongQuorumSignInfo = AuthorityQuorumSignInfo<true>;
pub type AuthorityWeakQuorumSignInfo = AuthorityQuorumSignInfo<false>;

// Note: if you meet an error due to this line it may be because you need an Eq implementation for `CertifiedTransaction`,
// or one of the structs that include it, i.e. `ConfirmationTransaction`, `TransactionInfoResponse` or `ObjectInfoResponse`.
//
// Please note that any such implementation must be agnostic to the exact set of signatures in the certificate, as
// clients are allowed to equivocate on the exact nature of valid certificates they send to the system. This assertion
// is a simple tool to make sure certificates are accounted for correctly - should you remove it, you're on your own to
// maintain the invariant that valid certificates with distinct signatures are equivalent, but yet-unchecked
// certificates that differ on signers aren't.
//
// see also https://github.com/MystenLabs/sui/issues/266
// static_assertions::assert_not_impl_any!(AuthorityStrongQuorumSignInfo<S>: Hash, Eq, PartialEq);
// static_assertions::assert_not_impl_any!(AuthorityWeakQuorumSignInfo<S>: Hash, Eq, PartialEq);

impl<const S: bool> AuthoritySignInfoTrait for AuthorityQuorumSignInfo<S> {}

impl<const STRONG_THRESHOLD: bool> AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new(epoch: EpochId) -> Self {
        AuthorityQuorumSignInfo {
            epoch,
            signature: AggregateAuthoritySignature::default(),
            signers_map: RoaringBitmap::new(),
        }
    }

    pub fn new_with_signatures(
        epoch: EpochId,
        mut signatures: Vec<(AuthorityPublicKeyBytes, AuthoritySignature)>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        let mut map = RoaringBitmap::new();

        signatures.sort_by_key(|(public_key, _)| *public_key);

        for (pk, _) in &signatures {
            map.insert(
                committee
                    .authority_index(pk)
                    .ok_or(SuiError::UnknownSigner)? as u32,
            );
        }
        let sigs: Vec<AuthoritySignature> = signatures.into_iter().map(|(_, sig)| sig).collect();

        Ok(AuthorityQuorumSignInfo {
            epoch,
            signature: AggregateAuthoritySignature::aggregate(sigs).map_err(|e| {
                SuiError::InvalidSignature {
                    error: e.to_string(),
                }
            })?,
            signers_map: map,
        })
    }

    pub fn authorities<'a>(
        &'a self,
        committee: &'a Committee,
    ) -> impl Iterator<Item = SuiResult<&AuthorityName>> {
        self.signers_map.iter().map(|i| {
            committee
                .authority_by_index(i)
                .ok_or(SuiError::InvalidAuthenticator)
        })
    }

    pub fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation<AggregateAuthoritySignature>,
        message_index: usize,
    ) -> SuiResult<()> {
        // Check epoch
        fp_ensure!(
            self.epoch == committee.epoch(),
            SuiError::WrongEpoch {
                expected_epoch: committee.epoch()
            }
        );

        let mut weight = 0;
        let pk_vec = obligation
            .public_keys
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAddress)?;

        // Create obligations for the committee signatures
        obligation
            .signatures
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAuthenticator)?
            .add_aggregate(self.signature.clone())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Signature Aggregation failed".to_string(),
            })?;

        for authority_index in self.signers_map.iter() {
            let authority = committee
                .authority_by_index(authority_index)
                .ok_or(SuiError::UnknownSigner)?;

            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
            weight += voting_rights;

            pk_vec.push(committee.public_key(authority)?);
        }

        let threshold = if STRONG_THRESHOLD {
            committee.quorum_threshold()
        } else {
            committee.validity_threshold()
        };
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);

        Ok(())
    }
}

mod private {
    pub trait SealedAuthoritySignInfoTrait {}
    impl SealedAuthoritySignInfoTrait for super::EmptySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthoritySignInfo {}
    impl<const S: bool> SealedAuthoritySignInfoTrait for super::AuthorityQuorumSignInfo<S> {}
}

/// Something that we know how to hash and sign.
pub trait Signable<W> {
    fn write(&self, writer: &mut W);
}
pub trait SignableBytes
where
    Self: Sized,
{
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error>;
}
/// Activate the blanket implementation of `Signable` based on serde and BCS.
/// * We use `serde_name` to extract a seed from the name of structs and enums.
/// * We use `BCS` to generate canonical bytes suitable for hashing and signing.
pub trait BcsSignable: Serialize + serde::de::DeserializeOwned {}

impl<T, W> Signable<W> for T
where
    T: BcsSignable,
    W: std::io::Write,
{
    fn write(&self, writer: &mut W) {
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        // Note: This assumes that names never contain the separator `::`.
        write!(writer, "{}::", name).expect("Hasher should not fail");
        bcs::serialize_into(writer, &self).expect("Message serialization should not fail");
    }
}

impl<T> SignableBytes for T
where
    T: BcsSignable,
{
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, Error> {
        // Remove name tag before deserialization using BCS
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        let name_byte_len = format!("{}::", name).bytes().len();
        Ok(bcs::from_bytes(&bytes[name_byte_len..])?)
    }
}

pub type PubKeyLookup<P> = HashMap<<P as VerifyingKey>::Bytes, P>;

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

#[derive(Default)]
pub struct VerificationObligation<S>
where
    S: AggregateAuthenticator,
{
    lookup: PubKeyLookup<S::PubKey>,
    pub messages: Vec<Vec<u8>>,
    pub signatures: Vec<S>,
    pub public_keys: Vec<Vec<S::PubKey>>,
}

impl<S: AggregateAuthenticator> VerificationObligation<S> {
    pub fn new(lookup: PubKeyLookup<S::PubKey>) -> VerificationObligation<S> {
        VerificationObligation {
            lookup,
            ..Default::default()
        }
    }

    pub fn lookup_public_key(
        &mut self,
        key_bytes: &<<S as AggregateAuthenticator>::PubKey as VerifyingKey>::Bytes,
    ) -> Result<S::PubKey, SuiError> {
        match self.lookup.get(key_bytes) {
            Some(v) => Ok(v.clone()),
            None => {
                let public_key: S::PubKey = (*key_bytes)
                    .try_into()
                    .map_err(|_| SuiError::InvalidAddress)?;
                self.lookup.insert(*key_bytes, public_key.clone());
                Ok(public_key)
            }
        }
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message(&mut self, message: Vec<u8>) -> usize {
        self.signatures.push(S::default());
        self.public_keys.push(Vec::new());
        self.messages.push(message);
        self.messages.len() - 1
    }

    pub fn verify_all(self) -> SuiResult<PubKeyLookup<S::PubKey>> {
        S::batch_verify(
            &self.signatures[..],
            &self.public_keys
                .iter()
                .map(|x| &x[..])
                .collect::<Vec<_>>(),
            &self.messages
                .iter()
                .map(|x| &x[..])
                .collect::<Vec<_>>()[..]
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;
        Ok(self.lookup)
    }
}
