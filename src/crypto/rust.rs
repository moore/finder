use super::*;

use rand_chacha::rand_core::SeedableRng;
use rsa::pkcs1v15::{SigningKey, VerifyingKey, Signature};
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
use rsa::RsaPrivateKey;
use rsa::RsaPublicKey;
use rsa::signature::Keypair;
use rsa::signature::Verifier;

use hkdf::Hkdf;
use rand_chacha::ChaCha20Rng;
use rsa::sha2::{Digest, Sha256};
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::traits::SignatureScheme;

use rand::Rng;

struct RustCrypto {
    rng: ChaCha20Rng,
}

impl RustCrypto {
    pub fn new( seed_bytes: &[u8]) -> Result<Self, CryptoError> {
        // BUG: we should make a salt for this use
        let hk = Hkdf::<Sha256>::new(None, seed_bytes);
        let mut seed = [0u8; 32];
        if hk.expand("'nonce seed".as_bytes(), &mut seed).is_err() {
            return Err(CryptoError::Unreachable);
        }

        let seed: <ChaCha20Rng as SeedableRng>::Seed = seed;
        let rng = ChaCha20Rng::from_seed(seed);
  
        Ok(Self{rng})
    }
}

impl From<rsa::Error> for CryptoError {
    fn from(_value: rsa::Error) -> Self {
        Self::VerifyError
    }
}

impl From<rsa::signature::Error> for CryptoError {
    fn from(_value: rsa::signature::Error) -> Self {
        Self::VerifyError
    }
}

impl Crypto for RustCrypto {
    type PubSigningKey = RsaPublicKey;
    type PrivateSigningKey = RsaPrivateKey;

    fn compute_id(key: &Self::PubSigningKey) -> NodeId {
        let Ok(encoded) = key.to_public_key_der() else {
            unimplemented!()
        };

        let mut hasher = Sha256::new();
        hasher.update(encoded.as_bytes());
        let result = hasher.finalize();
        let Ok(arr): Result<[u8; 32], _> = result.try_into() else {
            unimplemented!()
        };
        NodeId::new(arr)
    }

    fn envelope_id<T, const MAX_ENVELOPE: usize, const MAX_SIG: usize>(
        &self,
        sealed: &SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>,
    ) -> EnvelopeId {
        let mut hasher = Sha256::new();
        hasher.update(&sealed.serialized);
        hasher.update(&sealed.signature);
        let result = hasher.finalize();
        let Ok(arr): Result<[u8; 32], _> = result.try_into() else {
            unimplemented!()
        };
        EnvelopeId::new(arr)
    }

    fn seal<T: Serialize, const MAX_ENVELOPE: usize, const MAX_SIG: usize>(
        &self,
        key_pair: &KeyPair<Self::PrivateSigningKey, Self::PubSigningKey>,
        envelope: &Envelope<T>,
        target: &mut [u8],
    ) -> Result<SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>, CryptoError> {

        let serialized = to_slice(envelope, target)?;
        let mut hasher = Sha256::new();
        hasher.update(&serialized);
        let result = hasher.finalize();
        let Ok(envelope_hash): Result<[u8; 32], _> = result.try_into() else {
            unimplemented!()
        };

        let encoded = key_pair.private.to_pkcs8_der()?;

        // BUG: we should make a salt for this use
        let hk = Hkdf::<Sha256>::new(None, encoded.as_bytes());
        let mut seed = [0u8; 32];
        if hk.expand(&envelope_hash, &mut seed).is_err() {
            return Err(CryptoError::Unreachable);
        }

        let seed: <ChaCha20Rng as SeedableRng>::Seed = seed;
        let mut rng = ChaCha20Rng::from_seed(seed);

        let signing_key = SigningKey::<Sha256>::new(key_pair.private.clone());
        let signature = signing_key.sign_with_rng(&mut rng, &envelope_hash);

        let sig_bytes = signature.to_bytes();
        let sig_ref = sig_bytes.as_ref();
        let result = SealedEnvelope::new(serialized, sig_ref)?;

        Ok(result)
    }

    fn open<T: DeserializeOwned + Serialize, const MAX_ENVELOPE: usize, const MAX_SIG: usize>(
        &self,
        key: &Self::PubSigningKey,
        envelope: &SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>,
    ) -> Result<Envelope<T>, CryptoError> {

        let mut hasher = Sha256::new();
        hasher.update(&envelope.serialized);
        let envelope_hash = hasher.finalize();
   
        let verifying_key = VerifyingKey::<Sha256>::new(key.clone());
        let Ok(signature) = Signature::try_from(envelope.signature.as_ref()) else {
            return Err(CryptoError::InternalError);
        };

        verifying_key.verify(&envelope_hash, &signature)?;

        let opened = from_bytes(&envelope.serialized)?;
        
        Ok(opened)
    }

    fn nonce(&mut self) -> u128 {
        self.rng.gen()
    }

    fn make_signing_keys(&mut self) -> Result<KeyPair<Self::PrivateSigningKey, Self::PubSigningKey>, CryptoError> {
        let bits = 2048;

        let private_key = RsaPrivateKey::new(&mut self.rng, bits)?;

        let public_key = private_key.to_public_key();

        let key_pair = KeyPair {
            private: private_key,
            public: public_key,
        };
       
       Ok(key_pair)
    }
    
    fn channel_id_from_bytes(&self, data: &[u8]) -> ChannelId {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let Ok(arr): Result<[u8; 32], _> = result.try_into() else {
            unimplemented!()
        };

        ChannelId::new(arr)    }
}


#[cfg(test)]
mod test;